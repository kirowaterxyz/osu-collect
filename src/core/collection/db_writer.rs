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
    create_collection_db_entries(
        &[CollectionDbEntry {
            name: collection_name.to_string(),
            beatmap_hashes: collection_hashes(collection),
        }],
        output_dir,
    )
}

pub(crate) fn create_collection_db_entries(
    entries: &[CollectionDbEntry],
    output_dir: &Path,
) -> Result<()> {
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
pub fn generate_collection_folder_name(collection: &Collection) -> String {
    let sanitized_name = sanitize_filename(&collection.name);
    format!("{}-{}", sanitized_name, collection.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::collection::model::{Beatmap, Beatmapset, Collection};
    use tempfile::tempdir;

    fn make_collection(beatmapsets: Vec<Beatmapset>) -> Collection {
        use crate::core::collection::model::Uploader;
        Collection {
            id: 1,
            name: "test".into(),
            uploader: Uploader {
                id: 0,
                username: "".into(),
            },
            beatmapsets,
        }
    }

    fn make_beatmapset(id: u32, checksums: &[&str]) -> Beatmapset {
        Beatmapset {
            id,
            beatmaps: checksums
                .iter()
                .enumerate()
                .map(|(i, &cs)| Beatmap {
                    id: i as u32,
                    checksum: cs.into(),
                })
                .collect(),
        }
    }

    #[test]
    fn duplicate_hashes_written_once() {
        let shared_hash = "aabbccdd";
        let collection = make_collection(vec![
            make_beatmapset(1, &[shared_hash, "unique1"]),
            make_beatmapset(2, &[shared_hash, "unique2"]),
        ]);

        let dir = tempdir().unwrap();
        create_collection_db(&collection, "test", dir.path()).unwrap();

        let db_path = dir.path().join("collection.db");
        let list = osu_db::collection::CollectionList::from_file(&db_path).unwrap();
        let hashes: Vec<_> = list.collections[0]
            .beatmap_hashes
            .iter()
            .flatten()
            .collect();

        let shared_count = hashes.iter().filter(|h| h.as_str() == shared_hash).count();
        assert_eq!(shared_count, 1, "shared hash should appear exactly once");
        assert_eq!(hashes.len(), 3, "unique hashes should all be present");
    }

    #[test]
    fn no_duplicates_collection_unchanged() {
        let collection = make_collection(vec![
            make_beatmapset(1, &["hash1"]),
            make_beatmapset(2, &["hash2"]),
        ]);

        let dir = tempdir().unwrap();
        create_collection_db(&collection, "test", dir.path()).unwrap();

        let db_path = dir.path().join("collection.db");
        let list = osu_db::collection::CollectionList::from_file(&db_path).unwrap();
        let hashes: Vec<_> = list.collections[0]
            .beatmap_hashes
            .iter()
            .flatten()
            .collect();

        assert_eq!(hashes.len(), 2);
    }

    #[test]
    fn multiple_collections_are_written() {
        let dir = tempdir().unwrap();
        let entries = [
            CollectionDbEntry {
                name: "renamed collection - 10".to_string(),
                beatmap_hashes: vec!["hash1".to_string(), "hash2".to_string()],
            },
            CollectionDbEntry {
                name: "other collection - 20".to_string(),
                beatmap_hashes: vec!["hash2".to_string(), "hash3".to_string()],
            },
        ];

        create_collection_db_entries(&entries, dir.path()).unwrap();

        let db_path = dir.path().join("collection.db");
        let list = osu_db::collection::CollectionList::from_file(&db_path).unwrap();
        assert_eq!(list.collections.len(), 2);
        assert_eq!(
            list.collections[0].name.as_deref(),
            Some("renamed collection - 10")
        );
        assert_eq!(
            list.collections[1].name.as_deref(),
            Some("other collection - 20")
        );
        assert_eq!(list.collections[0].beatmap_hashes.len(), 2);
        assert_eq!(list.collections[1].beatmap_hashes.len(), 2);
        assert!(dir.path().join("osu!.name.cfg").exists());
    }
}
