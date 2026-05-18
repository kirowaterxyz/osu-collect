use super::{create_collection_db, write_entries};
use crate::core::collection::{CollectionDbEntry, test_beatmapset, test_collection};
use tempfile::tempdir;

#[test]
fn duplicate_hashes_written_once() {
    let shared_hash = "aabbccdd";
    let collection = test_collection(
        1,
        vec![
            test_beatmapset(1, &[shared_hash, "unique1"]),
            test_beatmapset(2, &[shared_hash, "unique2"]),
        ],
    );

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
    let collection = test_collection(
        1,
        vec![
            test_beatmapset(1, &["hash1"]),
            test_beatmapset(2, &["hash2"]),
        ],
    );

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

    write_entries(&entries, dir.path()).unwrap();

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
