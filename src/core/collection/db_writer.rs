use super::model::Collection;
use crate::{
    config::constants::OSU_DB_VERSION,
    utils::{AppError, Result, sanitize_filename},
};
use osu_db::collection::{Collection as DbCollection, CollectionList};
use std::{collections::HashSet, path::Path};

pub(crate) struct CollectionDbEntry {
    pub(crate) name: String,
    pub(crate) beatmap_hashes: Vec<String>,
}

/// Persist collection metadata to osu!'s collection.db format.
pub fn create_collection_db(
    collection: &Collection,
    collection_name: &str,
    output_dir: &Path,
) -> Result<()> {
    write_db_entries(
        &[CollectionDbEntry {
            name: collection_name.to_string(),
            beatmap_hashes: collection_hashes(collection),
        }],
        output_dir,
    )
}

pub(crate) fn write_db_entries(entries: &[CollectionDbEntry], output_dir: &Path) -> Result<()> {
    let collections = entries
        .iter()
        .map(|entry| {
            let mut seen = HashSet::new();
            DbCollection {
                name: Some(entry.name.clone()),
                beatmap_hashes: entry
                    .beatmap_hashes
                    .iter()
                    .filter(|hash| seen.insert((*hash).clone()))
                    .cloned()
                    .map(Some)
                    .collect(),
            }
        })
        .collect();

    write_collection_files(
        CollectionList {
            version: OSU_DB_VERSION,
            collections,
        },
        output_dir,
    )
}

fn collection_hashes(collection: &Collection) -> Vec<String> {
    collection
        .beatmapsets
        .iter()
        .flat_map(|beatmapset| {
            beatmapset
                .beatmaps
                .iter()
                .map(|beatmap| beatmap.checksum.to_string())
        })
        .collect()
}

fn write_collection_files(collection_list: CollectionList, output_dir: &Path) -> Result<()> {
    let db_path = output_dir.join("collection.db");
    collection_list.to_file(&db_path).map_err(|e| {
        AppError::other_dynamic(format!("failed to write collection.db: {e}").into_boxed_str())
    })?;

    let cfg_path = output_dir.join("osu!.name.cfg");
    std::fs::write(&cfg_path, "").map_err(|e| {
        AppError::other_dynamic(format!("failed to write osu!.name.cfg: {e}").into_boxed_str())
    })?;

    Ok(())
}

/// Generate the folder name that will host the downloaded beatmaps.
pub fn folder_name(collection: &Collection) -> String {
    let sanitized_name = sanitize_filename(&collection.name);
    format!("{}-{}", sanitized_name, collection.id)
}

#[cfg(test)]
#[path = "../../../tests/unit/core_db_writer.rs"]
mod tests;
