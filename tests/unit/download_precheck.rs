use osu_collect::download::precheck::{
    CacheKey, ValidationCache, extract_beatmapset_id, is_orphan_temp_name, scan_candidates,
};
use std::{collections::HashSet, ffi::OsStr, path::Path};
use tokio::sync::watch;

#[test]
fn detects_orphan_temp_files() {
    let yes = [
        "123.osz.part-12345-0",
        "abc.osz.part-1-9",
        "1 artist.osz.part-99999-42",
    ];
    let no = [
        "123.osz",
        "123.osz.part",
        "123.osz.part-abc-9",
        "123.osz.part-9-abc",
        "123.osz.part-9",
        "random.txt",
    ];
    for name in yes {
        assert!(
            is_orphan_temp_name(OsStr::new(name)),
            "expected match: {name}"
        );
    }
    for name in no {
        assert!(
            !is_orphan_temp_name(OsStr::new(name)),
            "expected no match: {name}"
        );
    }
}

#[test]
fn extracts_exact_prefixed_beatmapset_ids() {
    assert_eq!(extract_beatmapset_id(Path::new("123.osz")), Some(123));
    assert_eq!(
        extract_beatmapset_id(Path::new("123 artist.osz")),
        Some(123)
    );
    assert_eq!(extract_beatmapset_id(Path::new("1234.osz")), Some(1234));
    assert_eq!(extract_beatmapset_id(Path::new("123abc.osz")), None);
}

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
    let orphan = dir.path().join("789 artist.osz.part-1-2");
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
