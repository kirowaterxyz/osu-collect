use super::model::Collection;
use crate::{
    config::constants::OSU_DB_VERSION,
    utils::{AppError, Result, sanitize_filename},
};
use osu_db::collection::{Collection as DbCollection, CollectionList};
use std::{collections::HashSet, path::Path};

/// Persist collection metadata to osu!'s collection.db format.
pub fn create_collection_db(
    collection: &Collection,
    collection_name: &str,
    output_dir: &Path,
) -> Result<()> {
    let db_path = output_dir.join("collection.db");

    let mut seen: HashSet<String> = HashSet::new();
    let beatmap_hashes: Vec<Option<String>> = collection
        .beatmapsets
        .iter()
        .flat_map(|beatmapset| {
            beatmapset
                .beatmaps
                .iter()
                .map(|beatmap| beatmap.checksum.to_string())
        })
        .filter(|hash| seen.insert(hash.clone()))
        .map(Some)
        .collect();

    let db_collection = DbCollection {
        name: Some(collection_name.to_string()),
        beatmap_hashes,
    };

    let collection_list = CollectionList {
        version: OSU_DB_VERSION,
        collections: vec![db_collection],
    };

    collection_list.to_file(&db_path).map_err(|e| {
        AppError::other_dynamic(format!("Failed to write collection.db: {e}").into_boxed_str())
    })?;

    let cfg_path = output_dir.join("osu!.name.cfg");
    std::fs::write(&cfg_path, "").map_err(|e| {
        AppError::other_dynamic(format!("Failed to write osu!.name.cfg: {e}").into_boxed_str())
    })?;

    Ok(())
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
}

/// Generate the folder name that will host the downloaded beatmaps.
pub fn generate_collection_folder_name(collection: &Collection) -> String {
    let sanitized_name = sanitize_filename(&collection.name);
    format!("{}-{}", sanitized_name, collection.id)
}
