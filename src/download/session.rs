use super::{
    DownloadConfig, DownloadError, DownloadEvent, DownloadId, DownloadStage,
    SelectiveDownloadCollection,
    lock::{ActiveDownloadRegistry, DownloadLockGuard},
    precheck::{PrecheckOptions, PrecheckReport, verify_existing_beatmapsets},
};
use crate::{
    core::collection::{
        CollectionService, HttpCollectionService, folder_name,
        model::{Collection, Uploader},
    },
    utils::{self, prepare_directory},
};
use futures_util::{StreamExt, stream};
use std::{
    collections::HashSet,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
};
use tokio::{fs, sync::watch};
use tracing::{debug, info, warn};

pub(crate) struct OutputPreparation {
    pub(crate) output_dir: PathBuf,
    pub(crate) display: String,
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
    pub(crate) fn expectation_index(&self, beatmapset_ids: &[u32]) -> Arc<HashSet<u32>> {
        match self {
            SessionTarget::Collection(collection) => {
                Arc::new(collection.beatmapsets.iter().map(|s| s.id).collect())
            }
            SessionTarget::Selective { .. } => Arc::new(beatmapset_ids.iter().copied().collect()),
        }
    }

    pub(crate) fn announce_ready(
        &self,
        emit: &impl Fn(DownloadEvent),
        id: DownloadId,
        output: &OutputPreparation,
        beatmapset_ids: &[u32],
    ) {
        match self {
            SessionTarget::Collection(collection) => {
                emit(DownloadEvent::CollectionReady {
                    id,
                    collection_name: collection.name.to_string(),
                    uploader: collection.uploader.username.to_string(),
                    total_maps: collection.beatmapsets.len(),
                    output_dir: output.display.clone(),
                });
                emit(DownloadEvent::Log {
                    id,
                    message: format!("downloading to {}", output.display),
                });
            }
            SessionTarget::Selective {
                collection,
                collection_names,
                ..
            } => {
                emit(DownloadEvent::CollectionReady {
                    id,
                    collection_name: selective_collection_name(collection_names).to_string(),
                    uploader: collection.uploader.username.to_string(),
                    total_maps: collection.beatmapsets.len(),
                    output_dir: output.display.clone(),
                });
                emit(DownloadEvent::Log {
                    id,
                    message: format!("downloading updates to {}", output.display),
                });
            }
        }

        emit(DownloadEvent::BeatmapsRegistered {
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
    #[allow(dead_code)]
    pub(crate) id: DownloadId,
    pub(crate) target: SessionTarget,
    pub(crate) beatmapset_ids: Vec<u32>,
    pub(crate) pending_ids: Vec<u32>,
    pub(crate) initial_unverified: HashSet<u32>,
    pub(crate) initial_satisfied: HashSet<u32>,
    pub(crate) skipped_existing: u32,
    pub(crate) output: OutputPreparation,
    pub(crate) _lock_guard: DownloadLockGuard,
}

pub(crate) enum PrepareTarget<'a> {
    Collection {
        collection_input: &'a str,
    },
    Selective {
        collection_ids: &'a [u32],
        collections: Vec<SelectiveDownloadCollection>,
        beatmapset_ids: &'a [u32],
    },
}

pub(crate) struct PrepareParams<'a> {
    pub(crate) id: DownloadId,
    pub(crate) cancel_rx: watch::Receiver<bool>,
    pub(crate) config: &'a DownloadConfig,
    pub(crate) registry: &'a ActiveDownloadRegistry,
    pub(crate) emit: &'a (dyn Fn(DownloadEvent) + Send + Sync),
    pub(crate) target: PrepareTarget<'a>,
}

impl DownloadSession {
    pub(crate) async fn prepare(params: PrepareParams<'_>) -> Result<Option<Self>, DownloadError> {
        let directory = params.config.directory.as_str();
        let (target, output, beatmapset_ids) = match params.target {
            PrepareTarget::Collection { collection_input } => {
                let collection = resolve_collection(collection_input).await?;
                let mut beatmapset_ids: Vec<u32> =
                    collection.beatmapsets.iter().map(|b| b.id).collect();
                beatmapset_ids.sort_unstable();
                beatmapset_ids.dedup();
                let output = prepare_output_directory(directory, &collection).await?;
                (
                    SessionTarget::Collection(collection),
                    output,
                    beatmapset_ids,
                )
            }
            PrepareTarget::Selective {
                collection_ids,
                collections,
                beatmapset_ids,
            } => {
                let (collection, collections, collection_names) = resolve_selective_collections(
                    collection_ids,
                    collections,
                    beatmapset_ids,
                    params.id,
                    params.emit,
                )
                .await?;
                let output = prepare_selective_output(directory, collection_ids).await?;
                let mut target_ids = beatmapset_ids.to_vec();
                target_ids.sort_unstable();
                target_ids.dedup();
                (
                    SessionTarget::Selective {
                        collection,
                        collections,
                        collection_names,
                    },
                    output,
                    target_ids,
                )
            }
        };

        let lock_guard = DownloadLockGuard::acquire(&output.output_dir, params.registry)?;
        target.announce_ready(&params.emit, params.id, &output, &beatmapset_ids);

        Self::finalize(
            params.id,
            params.cancel_rx,
            target,
            beatmapset_ids,
            output,
            lock_guard,
            params.config,
            params.emit,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn finalize(
        id: DownloadId,
        cancel_rx: watch::Receiver<bool>,
        target: SessionTarget,
        beatmapset_ids: Vec<u32>,
        output: OutputPreparation,
        lock_guard: DownloadLockGuard,
        config: &DownloadConfig,
        emit: &(dyn Fn(DownloadEvent) + Send + Sync),
    ) -> Result<Option<Self>, DownloadError> {
        let expectations = target.expectation_index(&beatmapset_ids);
        emit(DownloadEvent::Log {
            id,
            message: "verifying existing beatmapsets on disk".into(),
        });
        emit(DownloadEvent::StageChanged {
            id,
            stage: DownloadStage::Rechecking,
        });

        let report = verify_existing_beatmapsets(
            id,
            &output.output_dir,
            expectations,
            config.concurrent.max(1) as usize,
            PrecheckOptions {
                notify_verified: true,
                verify_zip_eocd: config.verify_zip_eocd,
            },
            &cancel_rx,
            emit,
        )
        .await?;

        emit(DownloadEvent::StageChanged {
            id,
            stage: DownloadStage::Downloading,
        });

        if report.aborted {
            emit(DownloadEvent::Log {
                id,
                message: "download aborted during precheck".into(),
            });
            emit(DownloadEvent::Failed {
                id,
                message: "Download aborted by user".into(),
            });
            return Ok(None);
        }

        let PrecheckReport {
            satisfied,
            skipped,
            unverified,
            verified_bytes,
            ..
        } = report;

        let initial_unverified: HashSet<u32> = unverified.iter().copied().collect();

        if verified_bytes > 0 {
            emit(DownloadEvent::VerifiedMapSizes {
                id,
                total_bytes: verified_bytes,
            });
        }

        let pending_ids: Vec<u32> = beatmapset_ids
            .iter()
            .copied()
            .filter(|beatmap_id| !satisfied.contains(beatmap_id))
            .collect();

        emit(DownloadEvent::DownloadTarget {
            id,
            remaining: pending_ids.len(),
        });

        if skipped > 0 {
            emit(DownloadEvent::Log {
                id,
                message: format!("{skipped} beatmapsets already verified locally"),
            });
        }

        Ok(Some(Self {
            id,
            target,
            beatmapset_ids,
            pending_ids,
            initial_unverified,
            initial_satisfied: satisfied,
            skipped_existing: skipped,
            output,
            _lock_guard: lock_guard,
        }))
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

    let base_dir = prepare_directory(normalized).await?;
    debug!(base = %base_dir.display(), "validated base download directory");

    let output_dir = base_dir.join(folder_name);
    fs::create_dir_all(&output_dir).await?;
    let display_str = output_dir.to_string_lossy().to_string();
    info!(output_dir = %display_str, "prepared output directory");

    Ok(OutputPreparation {
        output_dir,
        display: display_str,
    })
}

async fn prepare_output_directory(
    directory: &str,
    collection: &Collection,
) -> Result<OutputPreparation, DownloadError> {
    let folder_name = folder_name(collection);
    prepare_output_dir_common(directory, &folder_name).await
}

pub(crate) async fn prepare_selective_output(
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
    let service = HttpCollectionService::create()?;
    let collection = service.fetch_collection(collection_id).await?;

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
    id: DownloadId,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) -> Result<(Collection, Vec<SelectiveDownloadCollection>, Vec<String>), DownloadError> {
    let service = HttpCollectionService::create()?;
    resolve_selective_with(
        &service,
        collection_ids,
        requested_collections,
        beatmapset_ids,
        id,
        emit,
    )
    .await
}

async fn resolve_selective_with<S>(
    service: &S,
    collection_ids: &[u32],
    requested_collections: Vec<SelectiveDownloadCollection>,
    beatmapset_ids: &[u32],
    id: DownloadId,
    emit: &(dyn Fn(DownloadEvent) + Send + Sync),
) -> Result<(Collection, Vec<SelectiveDownloadCollection>, Vec<String>), DownloadError>
where
    S: CollectionService,
{
    let target_set: HashSet<u32> = beatmapset_ids.iter().copied().collect();
    let total = collection_ids.len() as u32;
    emit(DownloadEvent::ResolveProgress {
        id,
        current: 0,
        total,
    });

    let progress = Arc::new(AtomicU32::new(0));
    let fetch_results: Vec<(u32, Result<_, _>)> = stream::iter(collection_ids.iter().copied())
        .map(|collection_id| {
            let progress = Arc::clone(&progress);
            async move {
                let result = service.fetch_collection(collection_id).await;
                let done = progress.fetch_add(1, Ordering::Relaxed) + 1;
                emit(DownloadEvent::ResolveProgress {
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
                let requested = requested_collections.iter().find(|c| c.id == collection_id);
                let collection_name = requested
                    .and_then(|c| (!c.name.is_empty()).then(|| c.name.clone()))
                    .unwrap_or_else(|| format!("{}-{}", collection.name, collection.id));
                let requested_ids: HashSet<u32> = requested
                    .map(|c| c.beatmapset_ids.iter().copied().collect())
                    .unwrap_or_default();
                let mut resolved = SelectiveDownloadCollection {
                    id: collection_id,
                    name: collection_name.clone(),
                    beatmapset_ids: Vec::new(),
                };

                collection_names.push(collection.name.to_string());

                for beatmapset in collection.beatmapsets {
                    if target_set.contains(&beatmapset.id) {
                        if requested_ids.contains(&beatmapset.id) {
                            resolved.beatmapset_ids.push(beatmapset.id);
                        }
                        if seen_beatmapset_ids.insert(beatmapset.id) {
                            selected_collection.beatmapsets.push(beatmapset);
                        }
                    }
                }

                if !resolved.beatmapset_ids.is_empty() {
                    resolved_collections.push(resolved);
                }
            }
            Err(err) => {
                warn!(
                    collection_id,
                    error = %err,
                    "skipping missing collection in selective download"
                );
            }
        }
    }

    selected_collection.name = selective_collection_name(&collection_names);

    if resolved_collections.is_empty() {
        return Err(DownloadError::EmptyCollection);
    }
    if selected_collection.beatmapsets.is_empty() {
        return Err(DownloadError::NoBeatmapsets);
    }

    Ok((selected_collection, resolved_collections, collection_names))
}

fn selective_collection_name(collection_names: &[String]) -> Box<str> {
    if collection_names.len() == 1 {
        format!("update: {}", collection_names[0]).into_boxed_str()
    } else {
        format!("update: {} collections", collection_names.len()).into_boxed_str()
    }
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
    async fn resolve_selective_dedupes_overlapping_beatmapsets() {
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
        let emit = |_event| {};
        let (selected, resolved, names) =
            resolve_selective_with(&service, &[1, 2], requested, &[10, 11, 12], 7, &emit)
                .await
                .expect("resolve must succeed");

        let mut bs_ids: Vec<u32> = selected.beatmapsets.iter().map(|b| b.id).collect();
        bs_ids.sort_unstable();
        assert_eq!(bs_ids, vec![10, 11, 12]);
        assert_eq!(names, vec!["alpha".to_string(), "beta".to_string()]);
        assert_eq!(resolved.len(), 2);
    }

    #[tokio::test]
    async fn resolve_selective_progress_is_monotonic() {
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
                    .unwrap();
                sleep(delay).await;
                Ok(c.clone())
            }
        }

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
        let events_inner = Arc::clone(&events);
        let emit = move |event: DownloadEvent| {
            if let DownloadEvent::ResolveProgress { current, .. } = event {
                events_inner.lock().unwrap().push(current);
            }
        };

        resolve_selective_with(&service, &[1, 2, 3], requested, &[10, 11, 12], 7, &emit)
            .await
            .expect("resolve must succeed");

        let observed = events.lock().unwrap().clone();
        assert_eq!(observed, vec![0, 1, 2, 3]);
    }
}
