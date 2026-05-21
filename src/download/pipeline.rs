use super::{
    DownloadConfig, DownloadError, DownloadEvent, DownloadHandle, DownloadId, DownloadRequest,
    DownloadStage, Emit, SelectiveDownloadRequest,
    collection_db::{write_collection_db, write_selective_collection_db},
    events::{Tally, emit_finish, translate_event},
    fetch_collection_sizes,
    lock::ActiveDownloadRegistry,
    session::{DownloadSession, PrepareParams, PrepareTarget},
    warn_low_disk_space,
};
use crate::{
    app::{download_history, download_history::DownloadHistoryEntry, snapshots},
    config::constants::{DEFAULT_PROGRESS_WATCHDOG_SECS, NETWORK_RETRY_CAP},
};
use futures_util::StreamExt;
use osu_downloader::{Downloader, Event as LibEvent, OnExists, Session as LibDownloadSession};
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

const ABORTED_LOG: &str = "download aborted before completion";
const ABORTED_FAIL: &str = "Download aborted by user";

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

type EmitArc = Arc<dyn Fn(DownloadEvent) + Send + Sync>;

fn spawn<F, Fut>(
    id: DownloadId,
    span: tracing::Span,
    tx: UnboundedSender<DownloadEvent>,
    runner: F,
) -> DownloadHandle
where
    F: FnOnce(watch::Receiver<bool>, EmitArc) -> Fut + Send + 'static,
    Fut: std::future::Future<Output = Result<(), DownloadError>> + Send,
{
    let (cancel_tx, cancel_rx) = watch::channel(false);
    let failure_tx = tx.clone();
    let emit: EmitArc = Arc::new(move |event: DownloadEvent| {
        let _ = tx.send(event);
    });

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

fn emit_resolving(id: DownloadId, emit: Emit<'_>) {
    emit(DownloadEvent::StageChanged {
        id,
        stage: DownloadStage::Resolving,
    });
}

async fn run_collection(
    id: DownloadId,
    request: DownloadRequest,
    cancel_rx: watch::Receiver<bool>,
    emit: EmitArc,
) -> Result<(), DownloadError> {
    let DownloadRequest {
        collection_input,
        config,
        auto_overwrite,
        // Carried into the pipeline for future use (e.g. logging the user's
        // pre-download retry decision). The library re-downloads the whole
        // collection either way, so no branching is required here.
        include_previously_failed: _,
    } = request;

    emit_resolving(id, emit.as_ref());

    let Some(session) = DownloadSession::prepare(PrepareParams {
        id,
        cancel_rx: cancel_rx.clone(),
        config: &config,
        registry: &DOWNLOAD_REGISTRY,
        emit: emit.as_ref(),
        target: PrepareTarget::Collection {
            collection_input: &collection_input,
        },
    })
    .await?
    else {
        return Ok(());
    };

    let collection = session.target.collection().clone();
    let output_dir = session.output.output_dir.clone();
    let pending_count = session.pending_ids.len();
    let history_id = collection.id;
    let history_name = collection.name.clone();

    let Some(tally) = run_pipeline_core(
        id,
        &session,
        &config,
        auto_overwrite,
        cancel_rx,
        emit.as_ref(),
    )
    .await?
    else {
        drop(session);
        try_remove_empty_output_dir(id, &output_dir, emit.as_ref()).await;
        return Ok(());
    };

    // collection.db reflects the full collection regardless of partial failures so that
    // saved state matches the user's intent even when some maps couldn't be downloaded.
    let db_collection_name = format!("{}-{}", collection.name, collection.id);
    write_collection_db(
        id,
        collection,
        db_collection_name,
        output_dir,
        emit.as_ref(),
    )
    .await?;

    if pending_count > 0 && !tally.successful.is_empty() {
        append_history(history_id, history_name, tally.successful.len());
    }

    emit_finish(id, emit.as_ref(), tally.to_summary());
    Ok(())
}

async fn run_selective(
    id: DownloadId,
    request: SelectiveDownloadRequest,
    cancel_rx: watch::Receiver<bool>,
    emit: EmitArc,
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

    emit_resolving(id, emit.as_ref());

    let Some(session) = DownloadSession::prepare(PrepareParams {
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
    .await?
    else {
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
    let pending_count = session.pending_ids.len();
    let history_id = collection.id;
    let history_name = collection.name.clone();

    let Some(tally) =
        run_pipeline_core(id, &session, &config, false, cancel_rx, emit.as_ref()).await?
    else {
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
        write_selective_collection_db(
            id,
            collection,
            selective_collections,
            verified_now.clone(),
            output_dir.clone(),
            emit.as_ref(),
        )
        .await?;
    }

    let all_targets_satisfied = target_ids.iter().all(|id| verified_now.contains(id));
    if all_targets_satisfied && let Some(snapshot_dir) = snapshot_dir {
        persist_snapshots(snapshot_dir, snapshot_files).await?;
    }

    if pending_count > 0 && !tally.successful.is_empty() {
        append_history(history_id, history_name, tally.successful.len());
    }

    emit_finish(id, emit.as_ref(), tally.to_summary());
    Ok(())
}

async fn persist_snapshots(
    snapshot_dir: PathBuf,
    snapshot_files: Vec<snapshots::CollectionSnapshotFile>,
) -> Result<(), DownloadError> {
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
    .map_err(|err| DownloadError::internal(format!("snapshot save task panicked: {err}")))
}

/// Drives the [`Downloader`] for the prepared session. Returns `None` if cancelled.
async fn run_pipeline_core(
    id: DownloadId,
    session: &DownloadSession,
    config: &DownloadConfig,
    auto_overwrite: bool,
    cancel_rx: watch::Receiver<bool>,
    emit: Emit<'_>,
) -> Result<Option<Tally>, DownloadError> {
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
    super::events::emit_overall_progress(id, &tally, emit);

    if session.pending_ids.is_empty() {
        return Ok(Some(tally));
    }

    let on_exists = if auto_overwrite {
        OnExists::Overwrite
    } else {
        OnExists::Skip
    };

    let downloader = Downloader::builder()
        .mirrors(config.mirrors.iter().cloned())
        .concurrent_downloads(config.concurrent.max(1) as usize)
        .archive_validation(config.archive_validation)
        .progress_timeout(Duration::from_secs(DEFAULT_PROGRESS_WATCHDOG_SECS))
        .network_retry_attempts(NETWORK_RETRY_CAP as usize)
        .on_exists(on_exists)
        .build()
        .map_err(|err| DownloadError::internal(err.to_string()))?;

    let mut session_handle = downloader.download_many(
        session.pending_ids.iter().copied(),
        &session.output.output_dir,
    );
    let mut events = session_handle
        .events()
        .expect("events() called once per session");
    let mut cancel_signal = cancel_rx;

    let cancelled = drive_session(
        &mut session_handle,
        &mut events,
        &mut cancel_signal,
        |lib_event| translate_event(id, lib_event, &mut tally, emit),
    )
    .await;

    let _ = session_handle.wait().await;

    if cancelled {
        emit(DownloadEvent::Log {
            id,
            message: ABORTED_LOG.into(),
        });
        emit(DownloadEvent::Failed {
            id,
            message: ABORTED_FAIL.into(),
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

async fn drive_session<F, S>(
    session_handle: &mut LibDownloadSession,
    events: &mut S,
    cancel_signal: &mut watch::Receiver<bool>,
    mut on_event: F,
) -> bool
where
    F: FnMut(LibEvent),
    S: futures_util::Stream<Item = LibEvent> + Unpin,
{
    loop {
        tokio::select! {
            biased;
            changed = cancel_signal.changed() => {
                if changed.is_err() { return false; }
                if *cancel_signal.borrow() {
                    session_handle.cancel();
                    return true;
                }
            }
            event = events.next() => match event {
                Some(lib_event) => on_event(lib_event),
                None => return false,
            },
        }
    }
}

fn append_history(collection_id: u32, name: String, count: usize) {
    let Some(path) = download_history::history_path() else {
        return;
    };
    let entry = DownloadHistoryEntry::new(collection_id, name, count);
    download_history::append(&path, entry);
}

pub async fn try_remove_empty_output_dir(id: DownloadId, output_dir: &Path, emit: Emit<'_>) {
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

#[cfg(test)]
#[path = "../../tests/unit/download_pipeline.rs"]
mod tests;
