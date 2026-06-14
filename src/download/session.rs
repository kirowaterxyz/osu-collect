use super::{
    DownloadConfig, DownloadError, DownloadEvent, DownloadId, DownloadStage,
    SelectiveDownloadCollection,
    lock::{ActiveDownloadRegistry, DownloadLockGuard},
    precheck::{PrecheckOptions, PrecheckReport, verify_existing_beatmapsets},
};
use crate::{
    core::collection::{Collection, CollectionService, HttpCollectionService, Uploader},
    utils::{self, prepare_directory},
};
use futures_util::{StreamExt, stream};
use osu_downloader::collection::CollectionClient;
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
        _beatmapset_ids: &[u32],
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
            }
        }
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
    pub(crate) emit: super::Emit<'a>,
    pub(crate) target: PrepareTarget<'a>,
    /// When set, precheck skips validation so every requested id stays pending
    /// and the library overwrites existing archives (`OnExists::Overwrite`).
    pub(crate) overwrite: bool,
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
                let output = prepare_output_dir(directory, &collection.folder_name()).await?;
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
                let service = HttpCollectionService::new(CollectionClient::new());
                let (collection, collections, collection_names) = resolve_selective_with(
                    &service,
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
            params.overwrite,
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
        overwrite: bool,
        emit: super::Emit<'_>,
    ) -> Result<Option<Self>, DownloadError> {
        let expectations = target.expectation_index(&beatmapset_ids);
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
                archive_validation: config.archive_validation,
                overwrite,
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

async fn prepare_output_dir(
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

async fn prepare_selective_output(
    directory: &str,
    collection_ids: &[u32],
) -> Result<OutputPreparation, DownloadError> {
    let folder_name = if collection_ids.len() == 1 {
        format!("update-{}", collection_ids[0])
    } else {
        format!("update-{}-collections", collection_ids.len())
    };
    prepare_output_dir(directory, &folder_name).await
}

async fn resolve_collection(collection_input: &str) -> Result<Collection, DownloadError> {
    let collection_id = utils::parse_collection_id(collection_input)?;
    let service = HttpCollectionService::new(CollectionClient::new());
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

pub(crate) async fn resolve_selective_with<S>(
    service: &S,
    collection_ids: &[u32],
    requested_collections: Vec<SelectiveDownloadCollection>,
    beatmapset_ids: &[u32],
    id: DownloadId,
    emit: super::Emit<'_>,
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
        name: "updates".to_string(),
        description: None,
        uploader: Uploader {
            id: 0,
            username: "updates".to_string(),
        },
        beatmapsets: Vec::new(),
        favourites: 0,
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

fn selective_collection_name(collection_names: &[String]) -> String {
    if collection_names.len() == 1 {
        format!("update: {}", collection_names[0])
    } else {
        format!("update: {} collections", collection_names.len())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/download_session.rs"]
mod tests;
