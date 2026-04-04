use super::{
    BeatmapTracker, DownloadError, DownloadEvent, DownloadId, DownloadStage, DownloadSummary,
    ShutdownToken,
    integrity::ExpectationIndex,
    lock::{ActiveDownloadRegistry, DownloadLockGuard},
    precheck::{PrecheckOptions, PrecheckReport, verify_existing_beatmapsets},
    status_helpers::{
        fail_status, log_status, progress_status, stage_status, target_status,
        verified_sizes_status,
    },
};
use crate::{
    core::collection::{
        CollectionService, HttpCollectionService, generate_collection_folder_name,
        model::Collection,
    },
    utils::{self, validate_and_prepare_directory},
    worker::StatusSink,
};
use dashmap::DashSet;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::fs;
use tracing::{debug, info, warn};

pub(crate) struct OutputPreparation {
    pub(crate) output_dir: PathBuf,
    pub(crate) display: String,
}

pub(crate) struct PrepareCollectionParams<'a> {
    pub(crate) id: DownloadId,
    pub(crate) status: StatusSink,
    pub(crate) shutdown: &'a ShutdownToken,
    pub(crate) directory: &'a str,
    pub(crate) collection_input: &'a str,
    pub(crate) thread_count: usize,
    pub(crate) verify_zip_eocd: bool,
    pub(crate) flavor: &'a PipelineFlavor,
    pub(crate) registry: &'a ActiveDownloadRegistry,
}

pub(crate) struct PrepareSelectiveParams<'a> {
    pub(crate) id: DownloadId,
    pub(crate) status: StatusSink,
    pub(crate) shutdown: &'a ShutdownToken,
    pub(crate) directory: &'a str,
    pub(crate) collection_ids: &'a [u32],
    pub(crate) beatmapset_ids: &'a [u32],
    pub(crate) thread_count: usize,
    pub(crate) verify_zip_eocd: bool,
    pub(crate) flavor: &'a PipelineFlavor,
    pub(crate) registry: &'a ActiveDownloadRegistry,
}

pub(crate) struct FinalizeSessionParams<'a> {
    pub(crate) id: DownloadId,
    pub(crate) status: StatusSink,
    pub(crate) shutdown: &'a ShutdownToken,
    pub(crate) target: SessionTarget,
    pub(crate) beatmapset_ids: Vec<u32>,
    pub(crate) output: OutputPreparation,
    pub(crate) lock_guard: DownloadLockGuard,
    pub(crate) thread_count: usize,
    pub(crate) verify_zip_eocd: bool,
    pub(crate) flavor: &'a PipelineFlavor,
}

pub(crate) enum SessionTarget {
    Collection(Collection),
    Selective { collection_names: Vec<String> },
}

impl SessionTarget {
    pub(crate) fn expectation_index(&self, beatmapset_ids: &[u32]) -> Arc<ExpectationIndex> {
        match self {
            SessionTarget::Collection(collection) => {
                Arc::new(ExpectationIndex::new(&collection.beatmapsets))
            }
            SessionTarget::Selective { .. } => Arc::new(ExpectationIndex::from_ids(beatmapset_ids)),
        }
    }

    pub(crate) fn announce_ready(
        &self,
        status: &StatusSink,
        id: DownloadId,
        output: &OutputPreparation,
        beatmapset_ids: &[u32],
    ) {
        match self {
            SessionTarget::Collection(collection) => {
                status.emit(DownloadEvent::CollectionReady {
                    id,
                    collection_name: collection.name.to_string(),
                    uploader: collection.uploader.username.to_string(),
                    total_maps: collection.beatmapsets.len(),
                    output_dir: output.display.clone(),
                });
                log_status(status, id, format!("Downloading to {}", output.display));
            }
            SessionTarget::Selective { collection_names } => {
                let collection_name = if collection_names.len() == 1 {
                    format!("Update: {}", collection_names[0])
                } else {
                    format!("Update: {} collections", collection_names.len())
                };
                status.emit(DownloadEvent::CollectionReady {
                    id,
                    collection_name,
                    uploader: "Updates".to_string(),
                    total_maps: 0,
                    output_dir: output.display.clone(),
                });
                log_status(
                    status,
                    id,
                    format!("Downloading updates to {}", output.display),
                );
            }
        }

        status.emit(DownloadEvent::BeatmapsRegistered {
            id,
            beatmap_ids: beatmapset_ids.to_vec(),
        });
    }

    pub(crate) fn collection(&self) -> Option<&Collection> {
        match self {
            SessionTarget::Collection(collection) => Some(collection),
            SessionTarget::Selective { .. } => None,
        }
    }
}

pub(crate) struct DownloadSession {
    pub(crate) id: DownloadId,
    pub(crate) status: StatusSink,
    pub(crate) target: SessionTarget,
    pub(crate) beatmapset_ids: Vec<u32>,
    pub(crate) output: OutputPreparation,
    pub(crate) tracker: BeatmapTracker,
    pub(crate) totals: DownloadSummary,
    pub(crate) initial_unverified: Arc<DashSet<u32>>,
    pub(crate) _lock_guard: DownloadLockGuard,
}

impl DownloadSession {
    pub(crate) async fn prepare_collection(
        params: PrepareCollectionParams<'_>,
    ) -> Result<Option<Self>, DownloadError> {
        let collection = resolve_collection(params.collection_input).await?;
        let beatmapset_ids: Vec<u32> = collection.beatmapsets.iter().map(|b| b.id).collect();
        let output = prepare_output_directory(params.directory, &collection).await?;
        let lock_guard = DownloadLockGuard::acquire(&output.output_dir, params.registry)?;
        let target = SessionTarget::Collection(collection);
        target.announce_ready(&params.status, params.id, &output, &beatmapset_ids);

        Self::finalize(FinalizeSessionParams {
            id: params.id,
            status: params.status,
            shutdown: params.shutdown,
            target,
            beatmapset_ids,
            output,
            lock_guard,
            thread_count: params.thread_count,
            verify_zip_eocd: params.verify_zip_eocd,
            flavor: params.flavor,
        })
        .await
    }

    pub(crate) async fn prepare_selective(
        params: PrepareSelectiveParams<'_>,
    ) -> Result<Option<Self>, DownloadError> {
        let collection_names =
            resolve_selective_collections(params.collection_ids, params.beatmapset_ids).await?;
        let output =
            prepare_selective_output_directory(params.directory, params.collection_ids).await?;
        let lock_guard = DownloadLockGuard::acquire(&output.output_dir, params.registry)?;
        let mut target_ids = params.beatmapset_ids.to_vec();
        target_ids.sort_unstable();
        let target = SessionTarget::Selective { collection_names };
        target.announce_ready(&params.status, params.id, &output, &target_ids);

        Self::finalize(FinalizeSessionParams {
            id: params.id,
            status: params.status,
            shutdown: params.shutdown,
            target,
            beatmapset_ids: target_ids,
            output,
            lock_guard,
            thread_count: params.thread_count,
            verify_zip_eocd: params.verify_zip_eocd,
            flavor: params.flavor,
        })
        .await
    }

    async fn finalize(params: FinalizeSessionParams<'_>) -> Result<Option<Self>, DownloadError> {
        let expectations = params.target.expectation_index(&params.beatmapset_ids);
        let precheck = perform_initial_precheck(
            &params.status,
            params.id,
            &params.output.output_dir,
            expectations,
            params.thread_count,
            params.verify_zip_eocd,
            params.shutdown,
        )
        .await?;

        if precheck.aborted {
            log_status(&params.status, params.id, params.flavor.precheck_abort_log);
            fail_status(&params.status, params.id, "Download aborted by user");
            return Ok(None);
        }

        if precheck.files_changed {
            log_status(
                &params.status,
                params.id,
                "Files changed during precheck; rescheduling affected beatmapsets",
            );
        }

        let PrecheckReport {
            satisfied,
            skipped,
            unverified,
            verified_bytes,
            ..
        } = precheck;

        let initial_unverified: Arc<DashSet<u32>> =
            Arc::new(DashSet::with_capacity(unverified.len()));
        for id in &unverified {
            initial_unverified.insert(*id);
        }

        if verified_bytes > 0 {
            verified_sizes_status(&params.status, params.id, verified_bytes);
        }

        let pending_ids: HashSet<u32> = params
            .beatmapset_ids
            .iter()
            .copied()
            .filter(|beatmap_id| !satisfied.contains(beatmap_id))
            .collect();
        let tracker = BeatmapTracker::with_verified(pending_ids.clone(), satisfied);

        target_status(&params.status, params.id, pending_ids.len());

        let totals = DownloadSummary {
            downloaded: 0,
            skipped,
            failed: 0,
            unverified: initial_unverified.len() as u32,
        };

        if totals.skipped > 0 {
            log_status(
                &params.status,
                params.id,
                format!("{} beatmapsets already verified locally", totals.skipped),
            );
            progress_status(&params.status, params.id, &totals);
        }

        Ok(Some(DownloadSession {
            id: params.id,
            status: params.status,
            target: params.target,
            beatmapset_ids: params.beatmapset_ids,
            output: params.output,
            tracker,
            totals,
            initial_unverified,
            _lock_guard: params.lock_guard,
        }))
    }
}

/// Configuration for how a download pipeline behaves.
#[derive(Clone, Copy, Debug)]
pub(crate) struct PipelineFlavor {
    pub(crate) precheck_abort_log: &'static str,
    pub(crate) abort_log_message: Option<&'static str>,
    pub(crate) abort_warning: Option<&'static str>,
    pub(crate) log_prefix: &'static str,
    pub(crate) failure_summary: &'static str,
    pub(crate) completion_log: &'static str,
}

impl PipelineFlavor {
    pub(crate) const fn collection() -> Self {
        Self {
            precheck_abort_log: "Download aborted during precheck",
            abort_log_message: Some("Download aborted before completion"),
            abort_warning: Some("Download aborted due to shutdown request"),
            log_prefix: "Starting",
            failure_summary: "Download completed with failed beatmapsets",
            completion_log: "Download pipeline finished and summary dispatched",
        }
    }

    pub(crate) const fn selective() -> Self {
        Self {
            precheck_abort_log: "Selective download aborted during precheck",
            abort_log_message: None,
            abort_warning: None,
            log_prefix: "Starting selective",
            failure_summary: "Selective download completed with failed beatmapsets",
            completion_log: "Selective download pipeline finished and summary dispatched",
        }
    }
}

pub(crate) async fn prepare_output_dir_common(
    base_path: &str,
    folder_name: &str,
) -> Result<OutputPreparation, DownloadError> {
    let normalized = {
        let trimmed = base_path.trim();
        if trimmed.is_empty() { "." } else { trimmed }
    };

    let base_dir = validate_and_prepare_directory(normalized).await?;
    debug!(base = %base_dir.display(), "Validated base download directory");

    let output_dir = base_dir.join(folder_name);
    fs::create_dir_all(&output_dir).await?;
    let output_dir_display = output_dir.to_string_lossy().to_string();
    info!(output_dir = %output_dir_display, "Prepared output directory");

    Ok(OutputPreparation {
        output_dir,
        display: output_dir_display,
    })
}

async fn prepare_output_directory(
    directory: &str,
    collection: &Collection,
) -> Result<OutputPreparation, DownloadError> {
    let folder_name = generate_collection_folder_name(collection);
    prepare_output_dir_common(directory, &folder_name).await
}

pub(crate) async fn prepare_selective_output_directory(
    directory: &str,
    collection_ids: &[u32],
) -> Result<OutputPreparation, DownloadError> {
    let folder_name = if collection_ids.len() == 1 {
        format!("update-{}", collection_ids[0])
    } else {
        format!("update-{}-collections", collection_ids.len())
    };
    prepare_output_dir_common(directory, &folder_name).await
}

async fn resolve_collection(collection_input: &str) -> Result<Collection, DownloadError> {
    let collection_id = utils::parse_collection_id(collection_input)?;
    debug!(collection_input = %collection_input, collection_id, "Parsed collection identifier");

    let collection_service = HttpCollectionService::builder().build()?;
    let collection = collection_service.fetch_collection(collection_id).await?;

    info!(
        collection_id,
        collection_name = %collection.name,
        total_maps = collection.beatmapsets.len(),
        "Fetched collection metadata"
    );

    if collection.beatmapsets.is_empty() {
        warn!(collection_id, "Collection contained no beatmaps");
        return Err(DownloadError::EmptyCollection);
    }

    Ok(collection)
}

pub(crate) async fn resolve_selective_collections(
    collection_ids: &[u32],
    beatmapset_ids: &[u32],
) -> Result<Vec<String>, DownloadError> {
    let collection_service = HttpCollectionService::builder().build()?;

    let mut collection_names = Vec::new();
    let target_set: HashSet<u32> = beatmapset_ids.iter().copied().collect();
    let mut matched_count = 0;

    for &collection_id in collection_ids {
        match collection_service.fetch_collection(collection_id).await {
            Ok(collection) => {
                collection_names.push(collection.name.to_string());

                for beatmapset in &collection.beatmapsets {
                    if target_set.contains(&beatmapset.id) {
                        matched_count += 1;
                    }
                }
            }
            Err(err) => {
                warn!(
                    collection_id,
                    error = %err,
                    "Skipping missing/inaccessible collection in selective download"
                );
            }
        }
    }

    info!(
        collection_count = collection_ids.len(),
        resolved_count = collection_names.len(),
        matched_beatmapsets = matched_count,
        "Resolved selective collections"
    );

    Ok(collection_names)
}

async fn perform_initial_precheck(
    status: &StatusSink,
    id: DownloadId,
    output_dir: &Path,
    expectations: Arc<ExpectationIndex>,
    thread_count: usize,
    verify_zip_eocd: bool,
    shutdown: &ShutdownToken,
) -> Result<PrecheckReport, DownloadError> {
    log_status(status, id, "Verifying existing beatmapsets on disk");
    stage_status(status, id, DownloadStage::Rechecking);
    info!("Starting disk precheck before downloads");
    let options = PrecheckOptions {
        verify_integrity: true,
        notify_verified: true,
        verify_zip_eocd,
    };
    let report = verify_existing_beatmapsets(
        id,
        output_dir,
        expectations,
        thread_count,
        options,
        shutdown,
        status,
    )
    .await?;
    if report.aborted {
        info!("Disk precheck aborted by shutdown");
    } else {
        info!(
            verified = report.satisfied.len(),
            skipped = report.skipped,
            "Finished initial disk precheck"
        );
    }
    stage_status(status, id, DownloadStage::Downloading);
    Ok(report)
}
