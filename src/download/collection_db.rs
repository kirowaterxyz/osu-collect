use super::{DownloadError, DownloadEvent, DownloadId, Emit, SelectiveDownloadCollection};
use crate::core::collection::{Collection, CollectionDbEntry, write_collections_db};
use crate::utils::AppError;
use std::{collections::HashSet, path::Path, path::PathBuf};
use tracing::error;

const COLLECTION_DB_CREATED: &str = "collection.db created successfully";
const COLLECTION_DB_FILENAME: &str = "collection.db";
const OSU_NAME_CFG_FILENAME: &str = "osu!.name.cfg";

pub async fn write_collection_db(
    id: DownloadId,
    collection: Collection,
    db_collection_name: String,
    output_dir: PathBuf,
    emit: Emit<'_>,
) -> Result<(), DownloadError> {
    run_blocking(move || create_collection_db(&collection, &db_collection_name, &output_dir))
        .await
        .map(|()| {
            emit(DownloadEvent::Log {
                id,
                message: COLLECTION_DB_CREATED.into(),
            })
        })
        .map_err(|err| {
            let message = format!("failed to create collection.db: {err}");
            emit(DownloadEvent::Log {
                id,
                message: message.clone(),
            });
            error!(error = %err, "failed to create collection.db");
            DownloadError::internal(message)
        })
}

pub async fn write_selective_collection_db(
    id: DownloadId,
    collection: Collection,
    collections: Vec<SelectiveDownloadCollection>,
    verified: HashSet<u32>,
    output_dir: PathBuf,
    emit: Emit<'_>,
) -> Result<(), DownloadError> {
    run_blocking(move || {
        create_selective_collection_database(&collection, &collections, &verified, &output_dir)
    })
    .await
    .map(|()| {
        emit(DownloadEvent::Log {
            id,
            message: COLLECTION_DB_CREATED.into(),
        })
    })
    .map_err(|err| {
        let message = format!("failed to create collection.db: {err}");
        emit(DownloadEvent::Log {
            id,
            message: message.clone(),
        });
        DownloadError::internal(message)
    })
}

async fn run_blocking<F, T>(f: F) -> Result<T, AppError>
where
    F: FnOnce() -> Result<T, AppError> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|err| {
            AppError::other_dynamic(format!("spawn_blocking panicked: {err}").into_boxed_str())
        })
        .and_then(|r| r)
}

pub fn create_collection_db(
    collection: &Collection,
    collection_name: &str,
    output_dir: &Path,
) -> Result<(), AppError> {
    write_entries(
        &[CollectionDbEntry {
            name: collection_name.to_string(),
            beatmap_hashes: collection
                .beatmapsets
                .iter()
                .flat_map(|beatmapset| beatmapset.beatmaps.iter().map(|b| b.checksum.clone()))
                .collect(),
        }],
        output_dir,
    )
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
                .flat_map(|beatmapset| beatmapset.beatmaps.iter().map(|b| b.checksum.clone()))
                .collect();
            (!hashes.is_empty()).then(|| CollectionDbEntry {
                name: selected.name.clone(),
                beatmap_hashes: hashes,
            })
        })
        .collect::<Vec<_>>();

    if entries.is_empty() {
        return Ok(());
    }
    write_entries(&entries, output_dir)
}

pub(crate) fn write_entries(
    entries: &[CollectionDbEntry],
    output_dir: &Path,
) -> Result<(), AppError> {
    let db_path = output_dir.join(COLLECTION_DB_FILENAME);
    write_collections_db(entries, &db_path).map_err(|e| {
        AppError::other_dynamic(format!("failed to write collection.db: {e}").into_boxed_str())
    })?;

    let cfg_path = output_dir.join(OSU_NAME_CFG_FILENAME);
    std::fs::write(&cfg_path, "").map_err(|e| {
        AppError::other_dynamic(format!("failed to write osu!.name.cfg: {e}").into_boxed_str())
    })?;

    Ok(())
}

#[cfg(test)]
#[path = "../../tests/unit/core_db_writer.rs"]
mod tests;
