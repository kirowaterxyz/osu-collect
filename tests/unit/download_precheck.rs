use super::{
    CacheKey, OszSnapshotEntry, ValidationCache, detect_changed_beatmapsets, scan_candidates,
};
use std::collections::HashSet;
use tokio::sync::watch;

#[test]
fn validation_cache_marks_and_lookups_by_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.osz");
    std::fs::write(&path, b"hello").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let cache = ValidationCache::default();
    let key = CacheKey::from_meta(&path, &meta);

    assert!(!cache.is_valid(&key), "miss before insert");
    cache.mark_valid(key.clone());
    assert!(cache.is_valid(&key), "hit after insert");

    std::fs::write(&path, b"changed-bytes").unwrap();
    let meta2 = std::fs::metadata(&path).unwrap();
    let key2 = CacheKey::from_meta(&path, &meta2);
    assert!(
        !cache.is_valid(&key2),
        "size change must invalidate the key"
    );
}

#[tokio::test]
async fn scans_expected_osz_candidates_and_removes_orphan_temps() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let expected = dir.path().join("123 artist.osz");
    let unexpected = dir.path().join("456 artist.osz");
    let orphan = dir.path().join("789 artist.osz.download-1-2.tmp");
    std::fs::write(&expected, b"expected").unwrap();
    std::fs::write(&unexpected, b"unexpected").unwrap();
    std::fs::write(&orphan, b"orphan").unwrap();

    let expectations: HashSet<u32> = [123].into_iter().collect();
    let (_tx, rx) = watch::channel(false);
    let scan = scan_candidates(dir.path(), &expectations, &rx)
        .await
        .expect("scan candidates");

    assert!(!scan.aborted);
    assert_eq!(scan.orphan_temp_count, 1);
    assert_eq!(scan.candidates.len(), 1);
    assert_eq!(scan.candidates[0].beatmapset_id, 123);
    assert_eq!(scan.candidates[0].path, expected);
    assert!(!orphan.exists());
}

// ── detect_changed_beatmapsets ────────────────────────────────────────────────

fn entry(name: &str, id: u32, size: u64, mtime: Option<u128>) -> OszSnapshotEntry {
    OszSnapshotEntry {
        name: name.into(),
        beatmapset_id: id,
        size,
        modified_micros: mtime,
    }
}

fn sorted(mut v: Vec<OszSnapshotEntry>) -> Vec<OszSnapshotEntry> {
    v.sort();
    v
}

fn ids(set: HashSet<u32>) -> Vec<u32> {
    let mut v: Vec<u32> = set.into_iter().collect();
    v.sort();
    v
}

#[test]
fn empty_both_returns_empty() {
    assert!(detect_changed_beatmapsets(&[], &[]).is_empty());
}

#[test]
fn empty_initial_all_final_are_added() {
    let fin = sorted(vec![
        entry("a.osz", 1, 100, None),
        entry("b.osz", 2, 200, None),
    ]);
    assert_eq!(ids(detect_changed_beatmapsets(&[], &fin)), vec![1, 2]);
}

#[test]
fn empty_final_all_initial_are_deleted() {
    let init = sorted(vec![
        entry("a.osz", 1, 100, None),
        entry("b.osz", 2, 200, None),
    ]);
    assert_eq!(ids(detect_changed_beatmapsets(&init, &[])), vec![1, 2]);
}

#[test]
fn identical_snapshots_no_changes() {
    let snap = sorted(vec![
        entry("a.osz", 1, 100, Some(1000)),
        entry("b.osz", 2, 200, Some(2000)),
        entry("c.osz", 3, 300, Some(3000)),
    ]);
    assert!(detect_changed_beatmapsets(&snap, &snap).is_empty());
}

#[test]
fn fully_disjoint_all_reported() {
    let init = sorted(vec![
        entry("a.osz", 1, 100, None),
        entry("b.osz", 2, 200, None),
    ]);
    let fin = sorted(vec![
        entry("c.osz", 3, 300, None),
        entry("d.osz", 4, 400, None),
    ]);
    assert_eq!(
        ids(detect_changed_beatmapsets(&init, &fin)),
        vec![1, 2, 3, 4]
    );
}

#[test]
fn partial_overlap_changed_detected() {
    let init = sorted(vec![
        entry("a.osz", 1, 100, Some(1000)),
        entry("b.osz", 2, 200, Some(2000)),
        entry("c.osz", 3, 300, Some(3000)),
    ]);
    // b mutated (size change), c unchanged, d added
    let fin = sorted(vec![
        entry("a.osz", 1, 100, Some(1000)),
        entry("b.osz", 2, 999, Some(2000)),
        entry("c.osz", 3, 300, Some(3000)),
        entry("d.osz", 4, 400, None),
    ]);
    assert_eq!(ids(detect_changed_beatmapsets(&init, &fin)), vec![2, 4]);
}

#[test]
fn mutation_at_start() {
    let init = sorted(vec![
        entry("a.osz", 1, 100, Some(1000)),
        entry("b.osz", 2, 200, Some(2000)),
    ]);
    let fin = sorted(vec![
        entry("a.osz", 1, 101, Some(1000)),
        entry("b.osz", 2, 200, Some(2000)),
    ]);
    assert_eq!(ids(detect_changed_beatmapsets(&init, &fin)), vec![1]);
}

#[test]
fn mutation_at_end() {
    let init = sorted(vec![
        entry("a.osz", 1, 100, Some(1000)),
        entry("b.osz", 2, 200, Some(2000)),
    ]);
    let fin = sorted(vec![
        entry("a.osz", 1, 100, Some(1000)),
        entry("b.osz", 2, 201, Some(2000)),
    ]);
    assert_eq!(ids(detect_changed_beatmapsets(&init, &fin)), vec![2]);
}

#[test]
fn mutation_via_mtime_change() {
    let init = sorted(vec![entry("a.osz", 1, 100, Some(1000))]);
    let fin = sorted(vec![entry("a.osz", 1, 100, Some(9999))]);
    assert_eq!(ids(detect_changed_beatmapsets(&init, &fin)), vec![1]);
}

#[test]
fn present_in_initial_not_final() {
    let init = sorted(vec![entry("only_init.osz", 7, 100, None)]);
    assert_eq!(ids(detect_changed_beatmapsets(&init, &[])), vec![7]);
}

#[test]
fn present_in_final_not_initial() {
    let fin = sorted(vec![entry("only_final.osz", 8, 100, None)]);
    assert_eq!(ids(detect_changed_beatmapsets(&[], &fin)), vec![8]);
}
