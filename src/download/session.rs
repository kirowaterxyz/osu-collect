use super::{
    BeatmapTracker, DownloadError, DownloadEvent, DownloadId, DownloadStage, DownloadSummary,
    SelectiveDownloadCollection, ShutdownToken,
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
        model::{Collection, Uploader},
    },
    utils::{self, validate_and_prepare_directory},
    worker::StatusSink,
};
use dashmap::DashSet;
use futures_util::{StreamExt, stream};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
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
    pub(crate) collections: Vec<SelectiveDownloadCollection>,
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
    Selective {
        collection: Collection,
        collections: Vec<SelectiveDownloadCollection>,
        collection_names: Vec<String>,
    },
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
                log_status(status, id, format!("downloading to {}", output.display));
            }
            SessionTarget::Selective {
                collection,
                collection_names,
                ..
            } => {
                status.emit(DownloadEvent::CollectionReady {
                    id,
                    collection_name: selective_collection_name(collection_names).to_string(),
                    uploader: collection.uploader.username.to_string(),
                    total_maps: collection.beatmapsets.len(),
                    output_dir: output.display.clone(),
                });
                log_status(
                    status,
                    id,
                    format!("downloading updates to {}", output.display),
                );
            }
        }

        status.emit(DownloadEvent::BeatmapsRegistered {
            id,
            beatmap_ids: beatmapset_ids.to_vec(),
        });
    }

    pub(crate) fn collection(&self) -> &Collection {
        match self {
            SessionTarget::Collection(collection) | SessionTarget::Selective { collection, .. } => {
                collection
            }
        }
    }

    pub(crate) fn selective_collections(&self) -> Option<&[SelectiveDownloadCollection]> {
        match self {
            SessionTarget::Collection(_) => None,
            SessionTarget::Selective { collections, .. } => Some(collections),
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
        let mut beatmapset_ids: Vec<u32> = collection.beatmapsets.iter().map(|b| b.id).collect();
        beatmapset_ids.sort_unstable();
        beatmapset_ids.dedup();
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
        let (collection, collections, collection_names) = resolve_selective_collections(
            params.collection_ids,
            params.collections,
            params.beatmapset_ids,
            &params.status,
            params.id,
        )
        .await?;
        let output =
            prepare_selective_output_directory(params.directory, params.collection_ids).await?;
        let lock_guard = DownloadLockGuard::acquire(&output.output_dir, params.registry)?;
        let mut target_ids = params.beatmapset_ids.to_vec();
        target_ids.sort_unstable();
        target_ids.dedup();
        let target = SessionTarget::Selective {
            collection,
            collections,
            collection_names,
        };
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
        "fetched collection metadata"
    );

    if collection.beatmapsets.is_empty() {
        warn!(collection_id, "collection contained no beatmaps");
        return Err(DownloadError::EmptyCollection);
    }

    Ok(collection)
}

const RESOLVE_CONCURRENCY: usize = 6;

pub(crate) async fn resolve_selective_collections(
    collection_ids: &[u32],
    requested_collections: Vec<SelectiveDownloadCollection>,
    beatmapset_ids: &[u32],
    status: &StatusSink,
    id: DownloadId,
) -> Result<(Collection, Vec<SelectiveDownloadCollection>, Vec<String>), DownloadError> {
    let service = HttpCollectionService::builder().build()?;
    resolve_selective_with(
        &service,
        collection_ids,
        requested_collections,
        beatmapset_ids,
        status,
        id,
    )
    .await
}

async fn resolve_selective_with<S>(
    service: &S,
    collection_ids: &[u32],
    requested_collections: Vec<SelectiveDownloadCollection>,
    beatmapset_ids: &[u32],
    status: &StatusSink,
    id: DownloadId,
) -> Result<(Collection, Vec<SelectiveDownloadCollection>, Vec<String>), DownloadError>
where
    S: CollectionService,
{
    let target_set: HashSet<u32> = beatmapset_ids.iter().copied().collect();

    let total = collection_ids.len() as u32;
    status.emit(DownloadEvent::ResolveProgress {
        id,
        current: 0,
        total,
    });

    // buffered() polls futures concurrently and yields results in input order, but
    // side effects fire when each future completes — which is non-deterministic. A
    // shared counter keeps the emitted "current" monotonic regardless of completion order.
    let progress = Arc::new(AtomicU32::new(0));

    let fetch_results: Vec<(u32, Result<_, _>)> = stream::iter(collection_ids.iter().copied())
        .map(|collection_id| {
            let progress = Arc::clone(&progress);
            async move {
                let result = service.fetch_collection(collection_id).await;
                let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                status.emit(DownloadEvent::ResolveProgress {
                    id,
                    current: done,
                    total,
                });
                (collection_id, result)
            }
        })
        .buffered(RESOLVE_CONCURRENCY)
        .collect()
        .await;

    let mut collection_names = Vec::with_capacity(fetch_results.len());
    let mut resolved_collections = Vec::with_capacity(fetch_results.len());
    let mut selected_collection = Collection {
        id: collection_ids.first().copied().unwrap_or_default(),
        name: "updates".into(),
        uploader: Uploader {
            id: 0,
            username: "updates".into(),
        },
        beatmapsets: Vec::new(),
    };
    let mut seen_beatmapset_ids: HashSet<u32> = HashSet::new();

    for (collection_id, result) in fetch_results {
        match result {
            Ok(collection) => {
                let requested_collection = requested_collections
                    .iter()
                    .find(|requested| requested.id == collection_id);
                let collection_name = requested_collection
                    .and_then(|requested| {
                        (!requested.name.is_empty()).then(|| requested.name.clone())
                    })
                    .unwrap_or_else(|| format!("{}-{}", collection.name, collection.id));
                let requested_ids: HashSet<u32> = requested_collection
                    .map(|requested| requested.beatmapset_ids.iter().copied().collect())
                    .unwrap_or_default();
                let mut resolved_collection = SelectiveDownloadCollection {
                    id: collection_id,
                    name: collection_name.clone(),
                    beatmapset_ids: Vec::new(),
                };

                collection_names.push(collection.name.to_string());

                for beatmapset in collection.beatmapsets {
                    if target_set.contains(&beatmapset.id) {
                        if requested_ids.contains(&beatmapset.id) {
                            resolved_collection.beatmapset_ids.push(beatmapset.id);
                        }
                        if seen_beatmapset_ids.insert(beatmapset.id) {
                            selected_collection.beatmapsets.push(beatmapset);
                        }
                    }
                }

                if !resolved_collection.beatmapset_ids.is_empty() {
                    resolved_collections.push(resolved_collection);
                }
            }
            Err(err) => {
                warn!(
                    collection_id,
                    error = %err,
                    "skipping missing/inaccessible collection in selective download"
                );
            }
        }
    }

    selected_collection.name = selective_collection_name(&collection_names);

    if resolved_collections.is_empty() {
        warn!(
            collection_count = collection_ids.len(),
            "no collections resolved in selective download"
        );
        return Err(DownloadError::EmptyCollection);
    }

    if selected_collection.beatmapsets.is_empty() {
        warn!(
            collection_count = collection_ids.len(),
            "no requested beatmapsets matched any fetched collection"
        );
        return Err(DownloadError::NoBeatmapsets);
    }

    info!(
        collection_count = collection_ids.len(),
        resolved_count = collection_names.len(),
        matched_beatmapsets = selected_collection.beatmapsets.len(),
        "resolved selective collections"
    );

    Ok((selected_collection, resolved_collections, collection_names))
}

fn selective_collection_name(collection_names: &[String]) -> Box<str> {
    if collection_names.len() == 1 {
        format!("update: {}", collection_names[0]).into_boxed_str()
    } else {
        format!("update: {} collections", collection_names.len()).into_boxed_str()
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::collection::model::{Beatmap, Beatmapset};
    use std::sync::Mutex;

    struct MockService {
        responses: Vec<(u32, Result<Collection, &'static str>)>,
    }

    impl CollectionService for MockService {
        async fn fetch_collection(&self, id: u32) -> utils::Result<Collection> {
            let response = self
                .responses
                .iter()
                .find(|(cid, _)| *cid == id)
                .map(|(_, r)| r.clone())
                .unwrap_or(Err("missing"));
            response.map_err(utils::AppError::other)
        }
    }

    fn beatmapset(id: u32) -> Beatmapset {
        Beatmapset {
            id,
            beatmaps: vec![Beatmap {
                id,
                checksum: "abc".into(),
            }],
        }
    }

    fn collection(id: u32, name: &str, ids: &[u32]) -> Collection {
        Collection {
            id,
            name: name.into(),
            uploader: Uploader {
                id: 0,
                username: "u".into(),
            },
            beatmapsets: ids.iter().copied().map(beatmapset).collect(),
        }
    }

    #[tokio::test]
    async fn resolve_selective_dedupes_overlapping_beatmapsets_and_preserves_order() {
        let service = MockService {
            responses: vec![
                (1, Ok(collection(1, "alpha", &[10, 11]))),
                (2, Ok(collection(2, "beta", &[10, 12]))),
            ],
        };
        let requested = vec![
            SelectiveDownloadCollection {
                id: 1,
                name: String::new(),
                beatmapset_ids: vec![10, 11],
            },
            SelectiveDownloadCollection {
                id: 2,
                name: String::new(),
                beatmapset_ids: vec![10, 12],
            },
        ];
        let events = Arc::new(Mutex::new(Vec::<(u32, u32)>::new()));
        let sink = {
            let events = Arc::clone(&events);
            StatusSink::from_fn(move |event| {
                if let DownloadEvent::ResolveProgress { current, total, .. } = event {
                    events.lock().unwrap().push((current, total));
                }
            })
        };

        let (selected, resolved, names) =
            resolve_selective_with(&service, &[1, 2], requested, &[10, 11, 12], &sink, 7)
                .await
                .expect("resolve must succeed");

        let bs_ids: Vec<u32> = selected.beatmapsets.iter().map(|b| b.id).collect();
        assert_eq!(bs_ids, vec![10, 11, 12], "dedup + ordered by input");
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(resolved.len(), 2);
        let observed = events.lock().unwrap().clone();
        assert!(observed.contains(&(0, 2)), "initial progress emitted");
        assert!(observed.contains(&(2, 2)), "final progress emitted");
    }

    #[tokio::test]
    async fn resolve_selective_progress_is_monotonic_under_concurrent_completion() {
        use std::time::Duration;
        use tokio::time::sleep;

        struct DelayedService {
            responses: Vec<(u32, Collection, Duration)>,
        }

        impl CollectionService for DelayedService {
            async fn fetch_collection(&self, id: u32) -> utils::Result<Collection> {
                let (_, ref c, delay) = *self
                    .responses
                    .iter()
                    .find(|(cid, _, _)| *cid == id)
                    .expect("known id");
                sleep(delay).await;
                Ok(c.clone())
            }
        }

        // first-issued future finishes LAST, exercising out-of-order completion.
        let service = DelayedService {
            responses: vec![
                (1, collection(1, "alpha", &[10]), Duration::from_millis(60)),
                (2, collection(2, "beta", &[11]), Duration::from_millis(10)),
                (3, collection(3, "gamma", &[12]), Duration::from_millis(30)),
            ],
        };
        let requested = vec![
            SelectiveDownloadCollection {
                id: 1,
                name: String::new(),
                beatmapset_ids: vec![10],
            },
            SelectiveDownloadCollection {
                id: 2,
                name: String::new(),
                beatmapset_ids: vec![11],
            },
            SelectiveDownloadCollection {
                id: 3,
                name: String::new(),
                beatmapset_ids: vec![12],
            },
        ];
        let events = Arc::new(Mutex::new(Vec::<u32>::new()));
        let sink = {
            let events = Arc::clone(&events);
            StatusSink::from_fn(move |event| {
                if let DownloadEvent::ResolveProgress { current, .. } = event {
                    events.lock().unwrap().push(current);
                }
            })
        };

        resolve_selective_with(&service, &[1, 2, 3], requested, &[10, 11, 12], &sink, 7)
            .await
            .expect("resolve must succeed");

        let observed = events.lock().unwrap().clone();
        assert_eq!(
            observed,
            vec![0, 1, 2, 3],
            "progress must be monotonic regardless of completion order"
        );
    }

    #[tokio::test]
    async fn resolve_selective_skips_failed_fetches() {
        let service = MockService {
            responses: vec![
                (1, Ok(collection(1, "alpha", &[10]))),
                (2, Err("offline")),
                (3, Ok(collection(3, "gamma", &[11]))),
            ],
        };
        let requested = vec![
            SelectiveDownloadCollection {
                id: 1,
                name: String::new(),
                beatmapset_ids: vec![10],
            },
            SelectiveDownloadCollection {
                id: 3,
                name: String::new(),
                beatmapset_ids: vec![11],
            },
        ];

        let (selected, resolved, _names) = resolve_selective_with(
            &service,
            &[1, 2, 3],
            requested,
            &[10, 11],
            &StatusSink::noop(),
            7,
        )
        .await
        .expect("partial resolve must succeed");

        let mut bs_ids: Vec<u32> = selected.beatmapsets.iter().map(|b| b.id).collect();
        bs_ids.sort_unstable();
        assert_eq!(bs_ids, vec![10, 11]);
        assert_eq!(resolved.len(), 2);
    }
}
