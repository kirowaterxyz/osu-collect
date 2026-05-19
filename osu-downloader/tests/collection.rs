use super::{Beatmapset, Collection, CollectionEntry, Uploader, write_collections_db};
use tempfile::tempdir;

fn sample_collection() -> Collection {
    Collection {
        id: 1,
        name: "Test".to_string(),
        description: None,
        uploader: Uploader {
            id: 1,
            username: "test".to_string(),
        },
        beatmapsets: vec![
            Beatmapset {
                id: 100,
                beatmaps: vec![],
            },
            Beatmapset {
                id: 200,
                beatmaps: vec![],
            },
        ],
        favourites: 0,
    }
}

#[test]
fn beatmapset_ids_preserves_order_and_deduplicates() {
    let mut collection = sample_collection();
    collection.beatmapsets.push(Beatmapset {
        id: 100,
        beatmaps: vec![],
    });
    assert_eq!(collection.beatmapset_ids(), vec![100, 200]);
}

#[test]
fn folder_name_sanitizes_and_appends_id() {
    let mut collection = sample_collection();
    collection.name = "weird / name *".to_string();
    assert_eq!(collection.folder_name(), "weird _ name _-1");
}

#[test]
fn folder_name_falls_back_when_name_is_blank() {
    let mut collection = sample_collection();
    collection.name = "   ".to_string();
    assert_eq!(collection.folder_name(), "collection-1");
}

#[test]
fn write_collections_db_dedup_empty_input() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let entry = CollectionEntry {
        name: "empty".to_string(),
        beatmap_hashes: vec![],
    };
    write_collections_db(&[entry], &path).unwrap();
    let list = osu_db::collection::CollectionList::from_file(&path).unwrap();
    assert_eq!(list.collections.len(), 1);
    assert_eq!(list.collections[0].beatmap_hashes.len(), 0);
}

#[test]
fn write_collections_db_dedup_all_unique() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let hashes: Vec<String> = (0..4).map(|i| format!("hash{i:032x}")).collect();
    let entry = CollectionEntry {
        name: "unique".to_string(),
        beatmap_hashes: hashes.clone(),
    };
    write_collections_db(&[entry], &path).unwrap();
    let list = osu_db::collection::CollectionList::from_file(&path).unwrap();
    let out: Vec<_> = list.collections[0]
        .beatmap_hashes
        .iter()
        .flatten()
        .collect();
    assert_eq!(out.len(), 4);
    for (i, h) in out.iter().enumerate() {
        assert_eq!(h.as_str(), hashes[i].as_str());
    }
}

#[test]
fn write_collections_db_dedup_all_duplicates() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let hash = "aabbccdd00112233aabbccdd00112233".to_string();
    let entry = CollectionEntry {
        name: "dups".to_string(),
        beatmap_hashes: vec![hash.clone(), hash.clone(), hash.clone()],
    };
    write_collections_db(&[entry], &path).unwrap();
    let list = osu_db::collection::CollectionList::from_file(&path).unwrap();
    let out: Vec<_> = list.collections[0]
        .beatmap_hashes
        .iter()
        .flatten()
        .collect();
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].as_str(), hash.as_str());
}
