use super::{
    BeatmapStage, DownloadConfig, DownloadError, DownloadEvent, DownloadHandle, DownloadId,
    DownloadRequest, DownloadStage, DownloadSummary, SelectiveDownloadCollection,
    SelectiveDownloadRequest,
    lock::ActiveDownloadRegistry,
    session::{DownloadSession, PrepareParams, PrepareTarget},
};
use crate::{
    app::snapshots,
    config::constants::{DEFAULT_PROGRESS_WATCHDOG_SECS, NETWORK_RETRY_CAP, status},
    core::collection::{
        CollectionDbEntry, create_collection_db, model::Collection, write_db_entries,
    },
    utils::{AppError, check_available_space, is_low_disk_space},
};
use futures_util::StreamExt;
use osu_downloader::{
    BeatmapsetStatusEvent, DownloadEvent as LibEvent, DownloadItem, Downloader, FileExistsPolicy,
    SkipReason, size::SizeFetcher,
};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock},
    time::Duration,
};
use tokio::{sync::mpsc::UnboundedSender, sync::watch};
use tracing::{Instrument, error, info, info_span, warn};

static DOWNLOAD_REGISTRY: LazyLock<ActiveDownloadRegistry> =
    LazyLock::new(ActiveDownloadRegistry::new);

pub fn spawn_download(
    id: DownloadId,
    request: DownloadRequest,
    tx: UnboundedSender<DownloadEvent>,
) -> DownloadHandle {
    let span = info_span!(
        "download_task",
        download_id = id,
        mirror_count = request.config.mirrors.len(),
        concurrent = request.config.concurrent
    );
    spawn(id, span, tx, move |cancel_rx, emit| async move {
        run_collection(id, request, cancel_rx, emit).await
    })
}

pub fn spawn_selective_download(
    id: DownloadId,
    request: SelectiveDownloadRequest,
    tx: UnboundedSender<DownloadEvent>,
) -> DownloadHandle {
    let span = info_span!(
        "selective_download_task",
        download_id = id,
        mirror_count = request.config.mirrors.len(),
        concurrent = request.config.concurrent,
        beatmapset_count = request.beatmapset_ids.len()
    );
    spawn(id, span, tx, move |cancel_rx, emit| async move {
        run_selective(id, request, cancel_rx, emit).await
    })
}

fn spawn<F, Fut>(
    id: DownloadId,
    span: tracing::Span,
    tx: UnboundedSender<DownloadEvent>,
    runner: F,
) -> DownloadHandle
where
    F: FnOnce(watch::Receiver<bool>, Arc<dyn Fn(DownloadEvent) + Send + Sync>) -> Fut
        + Send
        + 'static,
    Fut: std::future::Future<Output = Result<(), DownloadError>> + Send,
{
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let emit_tx = tx.clone();
    let emit: Arc<dyn Fn(DownloadEvent) + Send + Sync> = Arc::new(move |event: DownloadEvent| {
        let _ = emit_tx.send(event);
    });
    let failure_tx = tx;

    let join = tokio::spawn(
        async move {
            info!("download task started");
            if let Err(err) = runner(cancel_rx, emit).await {
                error!(error = %err, "download task failed");
                let _ = failure_tx.send(DownloadEvent::Failed {
                    id,
                    message: err.to_string(),
                });
            } else {
                info!("download task completed");
            }
        }
        .instrument(span),
    );

    DownloadHandle::new(cancel_tx, join)
}

async fn run_collection(
    id: DownloadId,
    request: DownloadRequest,
    cancel_rx: watch::Receiver<bool>,
    emit: Arc<dyn Fn(DownloadEvent) + Send + Sync>,
) -> Result<(), DownloadError> {
    let DownloadRequest {
        collection_input,
        config,
        auto_overwrite,
    } = request;

    emit(DownloadEvent::StageChanged {
        id,
        stage: DownloadStage::Resolving,
    });

    let session = DownloadSession::prepare(PrepareParams {
        id,
        cancel_rx: cancel_rx.clone(),
        config: &config,
        registry: &DOWNLOAD_REGISTRY,
        emit: emit.as_ref(),
        target: PrepareTarget::Collection {
            collection_input: &collection_input,
        },
    })
    .await?;

    let Some(session) = session else {
        return Ok(());
    };

    let collection_for_db = session.target.collection().clone();
    let output_dir = session.output.output_dir.clone();

    let outcome = run_pipeline_core(
        id,
        &session,
        &config,
        auto_overwrite,
        cancel_rx,
        emit.as_ref(),
    )
    .await?;

    let Some(tally) = outcome else {
        drop(session);
        try_remove_empty_output_dir(id, &output_dir, emit.as_ref()).await;
        return Ok(());
    };

    // collection.db reflects the full collection regardless of partial failures so that
    // saved state matches the user's intent even when some maps couldn't be downloaded.
    let collection = collection_for_db;
    let db_collection_name = format!("{}-{}", collection.name, collection.id);
    write_collection_db(
        id,
        collection,
        db_collection_name,
        output_dir,
        emit.as_ref(),
    )
    .await?;

    finish(id, emit.as_ref(), tally.to_summary());
    Ok(())
}

async fn run_selective(
    id: DownloadId,
    request: SelectiveDownloadRequest,
    cancel_rx: watch::Receiver<bool>,
    emit: Arc<dyn Fn(DownloadEvent) + Send + Sync>,
) -> Result<(), DownloadError> {
    let SelectiveDownloadRequest {
        collection_ids,
        beatmapset_ids,
        collections,
        config,
        snapshot_dir,
        snapshots: snapshot_files,
    } = request;

    if beatmapset_ids.is_empty() {
        return Err(DownloadError::NoBeatmapsets);
    }

    emit(DownloadEvent::StageChanged {
        id,
        stage: DownloadStage::Resolving,
    });

    let session = DownloadSession::prepare(PrepareParams {
        id,
        cancel_rx: cancel_rx.clone(),
        config: &config,
        registry: &DOWNLOAD_REGISTRY,
        emit: emit.as_ref(),
        target: PrepareTarget::Selective {
            collection_ids: &collection_ids,
            collections,
            beatmapset_ids: &beatmapset_ids,
        },
    })
    .await?;

    let Some(session) = session else {
        return Ok(());
    };

    let collection = session.target.collection().clone();
    let selective_collections = session
        .target
        .selective_collections()
        .map(<[_]>::to_vec)
        .unwrap_or_default();
    let output_dir = session.output.output_dir.clone();
    let initial_satisfied = session.initial_satisfied.clone();
    let target_ids = session.beatmapset_ids.clone();

    let outcome = run_pipeline_core(id, &session, &config, false, cancel_rx, emit.as_ref()).await?;

    let Some(tally) = outcome else {
        drop(session);
        try_remove_empty_output_dir(id, &output_dir, emit.as_ref()).await;
        return Ok(());
    };

    // every target that is verifiably on disk now: pre-existing + newly downloaded.
    let verified_now: HashSet<u32> = initial_satisfied
        .iter()
        .copied()
        .chain(tally.successful.iter().copied())
        .collect();

    if !verified_now.is_empty() {
        let collection_clone = collection.clone();
        let selective_clone = selective_collections.clone();
        let dir_clone = output_dir.clone();
        let verified_clone = verified_now.clone();
        let result = tokio::task::spawn_blocking(move || {
            create_selective_collection_database(
                &collection_clone,
                &selective_clone,
                &verified_clone,
                &dir_clone,
            )
        })
        .await
        .map_err(|e| DownloadError::internal(format!("spawn_blocking panicked: {e}")))
        .and_then(|r| r.map_err(|e| DownloadError::internal(e.to_string())));
        match result {
            Ok(()) => emit(DownloadEvent::Log {
                id,
                message: "collection.db created successfully".into(),
            }),
            Err(err) => {
                let message = format!("failed to create collection.db: {err}");
                emit(DownloadEvent::Log {
                    id,
                    message: message.clone(),
                });
                return Err(DownloadError::internal(message));
            }
        }
    }

    let all_targets_satisfied = target_ids.iter().all(|id| verified_now.contains(id));
    if all_targets_satisfied && let Some(snapshot_dir) = snapshot_dir {
        tokio::task::spawn_blocking(move || {
            for snapshot in snapshot_files {
                let Ok(collection_id) = snapshot.collection_id.parse() else {
                    continue;
                };
                snapshots::save(
                    &snapshot,
                    &snapshots::snapshot_path(&snapshot_dir, collection_id),
                );
            }
        })
        .await
        .map_err(|err| DownloadError::internal(format!("snapshot save task panicked: {err}")))?;
    }

    finish(id, emit.as_ref(), tally.to_summary());
    Ok(())
}

/// Outcome of [`run_pipeline_core`]: either the final tally, or `None` if the run was cancelled.
type PipelineOutcome = Option<Tally>;

/// Drives the [`Downloader`] for the prepared session and emits app events.
/// Returns `None` if the run was cancelled; otherwise the final [`Tally`].
async fn run_pipeline_core(
    id: DownloadId,
    session: &DownloadSession,
    config: &DownloadConfig,
    auto_overwrite: bool,
    cancel_rx: watch::Receiver<bool>,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) -> Result<PipelineOutcome, DownloadError> {
    if config.mirrors.is_empty() {
        return Err(DownloadError::NoMirrors);
    }

    fetch_collection_sizes(id, &session.beatmapset_ids, emit).await;
    warn_low_disk_space(id, &session.output.output_dir, emit);

    let mut tally = Tally {
        skipped: session.skipped_existing,
        unverified: session.initial_unverified.len() as u32,
        ..Tally::default()
    };
    emit_overall_progress(id, &tally, emit);

    if session.pending_ids.is_empty() {
        return Ok(Some(tally));
    }

    let policy = if auto_overwrite {
        FileExistsPolicy::OverwriteTarget
    } else {
        FileExistsPolicy::Skip
    };
    let items: Vec<DownloadItem> = session
        .pending_ids
        .iter()
        .copied()
        .map(|beatmapset_id| DownloadItem {
            beatmapset_id,
            policy,
        })
        .collect();

    let downloader = Downloader::builder()
        .mirrors(config.mirrors.iter().cloned())
        .concurrent_downloads(config.concurrent.max(1) as usize)
        .archive_validation(config.archive_validation)
        .progress_timeout(Duration::from_secs(DEFAULT_PROGRESS_WATCHDOG_SECS))
        .network_retry_attempts(NETWORK_RETRY_CAP as usize)
        .build()
        .map_err(|err| DownloadError::internal(err.to_string()))?;

    let mut session_handle = downloader.download_many(items, &session.output.output_dir);
    let mut events = session_handle.events();

    let mut cancel_signal = cancel_rx;

    let cancelled = loop {
        tokio::select! {
            biased;
            changed = cancel_signal.changed() => {
                if changed.is_err() { break false; }
                if *cancel_signal.borrow() {
                    session_handle.cancel();
                    break true;
                }
            }
            event = events.next() => {
                match event {
                    Some(lib_event) => translate_event(id, lib_event, &mut tally, emit),
                    None => break false,
                }
            }
        }
    };

    let _ = session_handle.wait().await;

    if cancelled {
        emit(DownloadEvent::Log {
            id,
            message: "download aborted before completion".into(),
        });
        emit(DownloadEvent::Failed {
            id,
            message: "Download aborted by user".into(),
        });
        return Ok(None);
    }

    if !tally.failures.is_empty() {
        emit(DownloadEvent::FailedMaps {
            id,
            failures: tally.failures.clone(),
        });
        warn!(
            count = tally.failures.len(),
            "download completed with failures"
        );
    }

    Ok(Some(tally))
}

#[derive(Default)]
pub struct Tally {
    pub downloaded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub unverified: u32,
    pub failures: Vec<(u32, String)>,
    /// Beatmapset IDs that this run downloaded successfully.
    pub successful: HashSet<u32>,
}

impl Tally {
    pub fn to_summary(&self) -> DownloadSummary {
        DownloadSummary {
            downloaded: self.downloaded,
            skipped: self.skipped,
            failed: self.failed,
            unverified: self.unverified,
        }
    }
}

pub fn translate_event(
    id: DownloadId,
    event: LibEvent,
    tally: &mut Tally,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) {
    match event {
        LibEvent::SessionStarted { total_beatmapsets } => {
            emit(DownloadEvent::Log {
                id,
                message: format!("downloading {total_beatmapsets} beatmapsets"),
            });
        }
        LibEvent::BeatmapsetStarted { .. } => {}
        LibEvent::BeatmapsetStatus {
            beatmapset_id,
            status,
        } => emit_status(id, beatmapset_id, status, emit),
        LibEvent::Progress {
            beatmapset_id,
            downloaded_bytes,
            total_bytes,
            ..
        } => {
            emit(DownloadEvent::BeatmapProgress {
                id,
                beatmapset_id,
                downloaded: downloaded_bytes,
                total: total_bytes.unwrap_or(0),
            });
        }
        LibEvent::BeatmapsetCompleted {
            beatmapset_id,
            mirror_used,
            verify_duration_us,
            ..
        } => {
            tally.downloaded = tally.downloaded.saturating_add(1);
            tally.successful.insert(beatmapset_id);
            if tally.unverified > 0 {
                tally.unverified = tally.unverified.saturating_sub(1);
            }
            emit(DownloadEvent::BeatmapStatus {
                id,
                beatmapset_id,
                stage: BeatmapStage::Success,
                message: format!("downloaded from {}", mirror_used.label()),
                rate_limited: false,
            });
            emit(DownloadEvent::BeatmapVerified {
                id,
                duration_us: verify_duration_us,
            });
            emit_overall_progress(id, tally, emit);
        }
        LibEvent::BeatmapsetSkipped {
            beatmapset_id,
            reason,
        } => match reason {
            SkipReason::AlreadyExists => {
                tally.skipped = tally.skipped.saturating_add(1);
                emit(DownloadEvent::BeatmapStatus {
                    id,
                    beatmapset_id,
                    stage: BeatmapStage::Skipped,
                    message: "skipped: already exists".to_string(),
                    rate_limited: false,
                });
                emit_overall_progress(id, tally, emit);
            }
            SkipReason::UnavailableOnMirrors | SkipReason::InvalidBeatmapsetId => {
                let message = match reason {
                    SkipReason::UnavailableOnMirrors => "unavailable on all mirrors".to_string(),
                    SkipReason::InvalidBeatmapsetId => "invalid beatmapset id".to_string(),
                    SkipReason::AlreadyExists => unreachable!(),
                };
                tally.failed = tally.failed.saturating_add(1);
                tally.failures.push((beatmapset_id, message.clone()));
                emit(DownloadEvent::BeatmapStatus {
                    id,
                    beatmapset_id,
                    stage: BeatmapStage::Failed,
                    message,
                    rate_limited: false,
                });
                emit_overall_progress(id, tally, emit);
            }
        },
        LibEvent::BeatmapsetFailed {
            beatmapset_id,
            error,
            ..
        } => {
            let reason = error.to_string();
            tally.failed = tally.failed.saturating_add(1);
            tally.failures.push((beatmapset_id, reason.clone()));
            emit(DownloadEvent::BeatmapStatus {
                id,
                beatmapset_id,
                stage: BeatmapStage::Failed,
                message: reason,
                rate_limited: false,
            });
            emit_overall_progress(id, tally, emit);
        }
        LibEvent::BeatmapsetNetworkError {
            beatmapset_id,
            reason,
        } => {
            warn!(beatmapset_id, %reason, "network error, all mirrors exhausted");
            let message = format!("network error: {reason}");
            tally.failed = tally.failed.saturating_add(1);
            tally.failures.push((beatmapset_id, message.clone()));
            emit(DownloadEvent::BeatmapStatus {
                id,
                beatmapset_id,
                stage: BeatmapStage::Failed,
                message,
                rate_limited: false,
            });
            emit_overall_progress(id, tally, emit);
        }
        LibEvent::SessionCompleted { .. } => {}
    }
}

fn emit_status(
    id: DownloadId,
    beatmapset_id: u32,
    event: BeatmapsetStatusEvent,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) {
    let (message, stage, rate_limited) = match event {
        // dont remove this
        BeatmapsetStatusEvent::Contacting { mirror } => (
            format!("checking {}", mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
        BeatmapsetStatusEvent::Downloading { mirror } => (
            format!("{} from {}", status::DOWNLOADING, mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
        BeatmapsetStatusEvent::Verifying { mirror } => (
            format!("verifying from {}", mirror.label()),
            BeatmapStage::Verifying,
            false,
        ),
        BeatmapsetStatusEvent::RateLimited { mirror, cooldown } => (
            format!(
                "{} on {}, waiting {}s",
                status::RATE_LIMITED,
                mirror.label(),
                cooldown.as_secs().max(1)
            ),
            BeatmapStage::Downloading,
            true,
        ),
        BeatmapsetStatusEvent::RetryingTransient {
            mirror,
            attempt,
            max_attempts,
            reason,
        } => (
            format!(
                "retrying {} after {reason} (attempt {attempt}/{max_attempts})",
                mirror.label()
            ),
            BeatmapStage::Downloading,
            false,
        ),
        BeatmapsetStatusEvent::MirrorFailed { mirror, reason } => (
            format!("{} failed: {reason}", mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
    };
    emit(DownloadEvent::BeatmapStatus {
        id,
        beatmapset_id,
        stage,
        message,
        rate_limited,
    });
}

fn emit_overall_progress(
    id: DownloadId,
    tally: &Tally,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) {
    emit(DownloadEvent::OverallProgress {
        id,
        downloaded: tally.downloaded,
        skipped: tally.skipped,
        failed: tally.failed,
        unverified: tally.unverified,
    });
}

fn finish(id: DownloadId, emit: &(dyn Fn(DownloadEvent) + Send + Sync), summary: DownloadSummary) {
    emit(DownloadEvent::Finished { id, summary });
    emit(DownloadEvent::StageChanged {
        id,
        stage: DownloadStage::Completed,
    });
}

fn warn_low_disk_space(
    id: DownloadId,
    output_dir: &Path,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) {
    if is_low_disk_space(output_dir)
        && let Some(available) = check_available_space(output_dir)
    {
        warn!(
            available_bytes = available,
            output_dir = %output_dir.display(),
            "low disk space detected"
        );
        emit(DownloadEvent::LowDiskSpace {
            id,
            available_bytes: available,
        });
    }
}

async fn fetch_collection_sizes(
    id: DownloadId,
    beatmapset_ids: &[u32],
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) {
    emit(DownloadEvent::Log {
        id,
        message: "fetching collection size from nekoha".into(),
    });
    let fetcher = match SizeFetcher::with_default_client() {
        Ok(f) => f,
        Err(err) => {
            warn!(error = %err, "failed to create size fetcher");
            return;
        }
    };
    let result = fetcher.fetch_sizes(beatmapset_ids).await;
    emit(DownloadEvent::CollectionSizeResolved {
        id,
        total_bytes: result.total_bytes,
    });
    if result.missing_count > 0 {
        emit(DownloadEvent::Log {
            id,
            message: format!(
                "size info unavailable for {} beatmapsets",
                result.missing_count
            ),
        });
    }
}

async fn write_collection_db(
    id: DownloadId,
    collection: Collection,
    db_collection_name: String,
    output_dir: PathBuf,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) -> Result<(), DownloadError> {
    let result = tokio::task::spawn_blocking(move || {
        create_collection_db(&collection, &db_collection_name, &output_dir)
    })
    .await
    .map_err(|e| AppError::other_dynamic(format!("spawn_blocking panicked: {e}").into_boxed_str()))
    .and_then(|r| r);
    match result {
        Ok(()) => {
            emit(DownloadEvent::Log {
                id,
                message: "collection.db created successfully".into(),
            });
            Ok(())
        }
        Err(err) => {
            let message = format!("failed to create collection.db: {err}");
            emit(DownloadEvent::Log {
                id,
                message: message.clone(),
            });
            error!(error = %err, "failed to create collection.db");
            Err(DownloadError::internal(message))
        }
    }
}

pub async fn try_remove_empty_output_dir(
    id: DownloadId,
    output_dir: &Path,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) {
    let Ok(mut entries) = tokio::fs::read_dir(output_dir).await else {
        return;
    };
    if entries.next_entry().await.ok().flatten().is_some() {
        return;
    }
    if let Err(err) = tokio::fs::remove_dir(output_dir).await {
        warn!(error = %err, path = %output_dir.display(), "failed to remove empty output directory");
        return;
    }
    emit(DownloadEvent::Log {
        id,
        message: format!("removed empty directory {}", output_dir.display()),
    });
}

pub fn create_selective_collection_database(
    collection: &Collection,
    collections: &[SelectiveDownloadCollection],
    newly_downloaded: &HashSet<u32>,
    output_dir: &Path,
) -> Result<(), AppError> {
    let entries = collections
        .iter()
        .filter_map(|selected| {
            let hashes: Vec<String> = collection
                .beatmapsets
                .iter()
                .filter(|beatmapset| {
                    selected.beatmapset_ids.contains(&beatmapset.id)
                        && newly_downloaded.contains(&beatmapset.id)
                })
                .flat_map(|beatmapset| {
                    beatmapset
                        .beatmaps
                        .iter()
                        .map(|beatmap| beatmap.checksum.to_string())
                })
                .collect();
            if hashes.is_empty() {
                None
            } else {
                Some(CollectionDbEntry {
                    name: selected.name.clone(),
                    beatmap_hashes: hashes,
                })
            }
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        return Ok(());
    }
    write_db_entries(&entries, output_dir)
}
