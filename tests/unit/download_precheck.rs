use super::{
    CacheKey, Candidate, PrecheckOptions, ValidationCache, scan_candidates,
    validate_existing_candidate, verify_existing_beatmapsets,
};
use osu_downloader::ArchiveValidation;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::watch;

#[test]
fn validation_cache_marks_and_lookups_by_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.osz");
    std::fs::write(&path, b"hello").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let cache = ValidationCache::default();
    let key = CacheKey::from_meta(&path, &meta);

    assert!(
        !cache.is_valid(&key, ArchiveValidation::Magic),
        "miss before insert"
    );
    cache.mark_valid(key.clone(), ArchiveValidation::Magic);
    assert!(
        cache.is_valid(&key, ArchiveValidation::Magic),
        "hit after insert"
    );

    std::fs::write(&path, b"changed-bytes").unwrap();
    let meta2 = std::fs::metadata(&path).unwrap();
    let key2 = CacheKey::from_meta(&path, &meta2);
    assert!(
        !cache.is_valid(&key2, ArchiveValidation::Magic),
        "size change must invalidate the key"
    );
}

#[test]
fn validation_cache_off_mode_does_not_insert() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.osz");
    std::fs::write(&path, b"hello").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let cache = ValidationCache::default();
    let key = CacheKey::from_meta(&path, &meta);

    cache.mark_valid(key.clone(), ArchiveValidation::Off);
    assert!(
        !cache.is_valid(&key, ArchiveValidation::Off),
        "Off must not populate the cache"
    );
}

#[test]
fn validation_cache_strict_request_misses_weaker_entry() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.osz");
    std::fs::write(&path, b"hello").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let cache = ValidationCache::default();
    let key = CacheKey::from_meta(&path, &meta);

    cache.mark_valid(key.clone(), ArchiveValidation::Magic);
    assert!(
        !cache.is_valid(&key, ArchiveValidation::Eocd),
        "stored Magic must not satisfy an Eocd lookup"
    );
    assert!(
        cache.is_valid(&key, ArchiveValidation::Magic),
        "stored Magic must satisfy a Magic lookup"
    );
}

#[test]
fn validation_cache_eocd_entry_satisfies_weaker_lookups() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.osz");
    std::fs::write(&path, b"hello").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let cache = ValidationCache::default();
    let key = CacheKey::from_meta(&path, &meta);

    cache.mark_valid(key.clone(), ArchiveValidation::Eocd);
    assert!(cache.is_valid(&key, ArchiveValidation::Magic));
    assert!(cache.is_valid(&key, ArchiveValidation::Eocd));
}

#[test]
fn validation_cache_upgrades_to_stricter_on_remark() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123.osz");
    std::fs::write(&path, b"hello").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    let cache = ValidationCache::default();
    let key = CacheKey::from_meta(&path, &meta);

    cache.mark_valid(key.clone(), ArchiveValidation::Magic);
    cache.mark_valid(key.clone(), ArchiveValidation::Eocd);
    assert!(
        cache.is_valid(&key, ArchiveValidation::Eocd),
        "second mark must upgrade strictness"
    );

    cache.mark_valid(key.clone(), ArchiveValidation::Magic);
    assert!(
        cache.is_valid(&key, ArchiveValidation::Eocd),
        "weaker mark must not downgrade strictness"
    );
}

#[tokio::test]
async fn off_mode_accepts_non_empty_file_without_validating() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123 artist.osz");
    // intentionally not a real ZIP — Off mode must accept it
    std::fs::write(&path, b"not a real zip but non-empty").unwrap();

    let (_tx, rx) = watch::channel(false);
    let record = validate_existing_candidate(
        Candidate {
            path: path.clone(),
            beatmapset_id: 123,
        },
        PrecheckOptions {
            notify_verified: false,
            archive_validation: ArchiveValidation::Off,
            overwrite: false,
        },
        rx,
    )
    .await
    .expect("validate succeeds")
    .expect("record returned");

    assert_eq!(record.beatmapset_id, 123);
    assert!(record.validation_error.is_none());
    assert_eq!(record.file_size, 28);
    assert!(path.exists(), "Off mode must not delete a valid file");
}

#[tokio::test]
async fn off_mode_deletes_empty_file_and_flags_invalid() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("123 artist.osz");
    std::fs::write(&path, b"").unwrap();

    let (_tx, rx) = watch::channel(false);
    let record = validate_existing_candidate(
        Candidate {
            path: path.clone(),
            beatmapset_id: 123,
        },
        PrecheckOptions {
            notify_verified: false,
            archive_validation: ArchiveValidation::Off,
            overwrite: false,
        },
        rx,
    )
    .await
    .expect("validate succeeds")
    .expect("record returned");

    assert!(record.validation_error.is_some(), "empty must be flagged");
    assert!(!path.exists(), "Off mode must delete empty files");
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

/// Overwrite mode never marks an existing valid archive as satisfied, so every
/// requested id is left pending for re-download — while orphan temps are still
/// swept during the scan.
#[tokio::test]
async fn overwrite_mode_marks_nothing_satisfied_and_removes_orphans() {
    let dir = tempfile::tempdir().expect("create tempdir");
    let existing = dir.path().join("123 artist.osz");
    let orphan = dir.path().join("123 artist.osz.download-1-2.tmp");
    std::fs::write(&existing, b"a valid-looking existing archive").unwrap();
    std::fs::write(&orphan, b"orphan").unwrap();

    let expectations: Arc<HashSet<u32>> = Arc::new([123, 456].into_iter().collect());
    let (_tx, rx) = watch::channel(false);
    let report = verify_existing_beatmapsets(
        0,
        dir.path(),
        expectations,
        1,
        PrecheckOptions {
            notify_verified: false,
            archive_validation: ArchiveValidation::Eocd,
            overwrite: true,
        },
        &rx,
        |_event| {},
    )
    .await
    .expect("precheck succeeds");

    assert!(
        report.satisfied.is_empty(),
        "overwrite mode must not classify any id as satisfied"
    );
    assert_eq!(report.skipped, 0, "nothing is skipped under overwrite");
    assert!(report.unverified.is_empty());
    assert_eq!(report.verified_bytes, 0);
    assert!(!report.aborted);
    assert!(
        !orphan.exists(),
        "orphan temp files are still removed in overwrite mode"
    );
    assert!(
        existing.exists(),
        "precheck must leave the existing archive on disk; the downloader deletes it"
    );
}
