use crate::app::download_history::{DownloadHistoryEntry, append, load};

// ── roundtrip ────────────────────────────────────────────────────────────────

#[test]
fn load_missing_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("download-history.json");

    let entries = load(&path);

    assert!(entries.is_empty());
}

#[test]
fn load_corrupt_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("download-history.json");
    std::fs::write(&path, b"not valid json {{{{").unwrap();

    let entries = load(&path);

    assert!(entries.is_empty());
}

#[test]
fn append_then_load_contains_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("download-history.json");

    let entry = DownloadHistoryEntry::new(12345, "ranked maps 2024".to_string(), 432);
    append(&path, entry);

    let entries = load(&path);

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].collection_id, 12345);
    assert_eq!(entries[0].name, "ranked maps 2024");
    assert_eq!(entries[0].count, 432);
    // completed_at must be a non-empty ISO-8601 string
    assert!(!entries[0].completed_at.is_empty());
}

#[test]
fn append_multiple_grows_list() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("download-history.json");

    append(&path, DownloadHistoryEntry::new(1, "first".to_string(), 10));
    append(
        &path,
        DownloadHistoryEntry::new(2, "second".to_string(), 20),
    );
    append(&path, DownloadHistoryEntry::new(3, "third".to_string(), 30));

    let entries = load(&path);

    assert_eq!(entries.len(), 3);
    assert_eq!(entries[0].collection_id, 1);
    assert_eq!(entries[1].collection_id, 2);
    assert_eq!(entries[2].collection_id, 3);
}

#[test]
fn append_preserves_existing_entries() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("download-history.json");

    append(
        &path,
        DownloadHistoryEntry::new(100, "original".to_string(), 5),
    );
    let before = load(&path);
    assert_eq!(before.len(), 1);

    append(
        &path,
        DownloadHistoryEntry::new(200, "added later".to_string(), 15),
    );
    let after = load(&path);

    assert_eq!(after.len(), 2);
    assert_eq!(after[0].collection_id, 100);
    assert_eq!(after[1].collection_id, 200);
}

// ── serde shape ──────────────────────────────────────────────────────────────

#[test]
fn serialized_shape_matches_spec() {
    let entry = DownloadHistoryEntry {
        collection_id: 99,
        name: "test collection".to_string(),
        completed_at: "2026-05-21T14:32:00Z".to_string(),
        count: 7,
    };
    let json = serde_json::to_value(&entry).unwrap();

    assert_eq!(json["collection_id"], 99u64);
    assert_eq!(json["name"], "test collection");
    assert_eq!(json["completed_at"], "2026-05-21T14:32:00Z");
    assert_eq!(json["count"], 7u64);
}

#[test]
fn deserialize_from_spec_json() {
    let json = r#"[
        {
            "collection_id": 12345,
            "name": "ranked maps 2024",
            "completed_at": "2026-05-21T14:32:00Z",
            "count": 432
        }
    ]"#;
    let entries: Vec<DownloadHistoryEntry> = serde_json::from_str(json).unwrap();

    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].collection_id, 12345);
    assert_eq!(entries[0].name, "ranked maps 2024");
    assert_eq!(entries[0].completed_at, "2026-05-21T14:32:00Z");
    assert_eq!(entries[0].count, 432);
}
