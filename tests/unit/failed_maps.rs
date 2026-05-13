use osu_collect::app::failed_maps::{
    FailedMapsFile, failed_maps_path_from_base, load, record_failures, remove_available, save,
};
use std::collections::HashSet;

#[test]
fn load_missing_file_returns_empty() {
    let dir = tempfile::tempdir().unwrap();

    let failed_maps = load(&dir.path().join("missing.json"));

    assert_eq!(failed_maps.beatmapset_ids, Vec::<u32>::new());
}

#[test]
fn save_and_load_sorts_and_dedupes_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("failed.json");
    let failed_maps = FailedMapsFile {
        schema_version: 0,
        beatmapset_ids: vec![30, 10, 30, 20],
    };

    save(&failed_maps, &path);
    let loaded = load(&path);

    assert_eq!(loaded.schema_version, 1);
    assert_eq!(loaded.beatmapset_ids, vec![10, 20, 30]);
}

#[test]
fn record_failures_merges_existing_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("failed.json");
    record_failures(&path, [3, 1]);
    record_failures(&path, [2, 3]);

    let loaded = load(&path);

    assert_eq!(loaded.beatmapset_ids, vec![1, 2, 3]);
}

#[test]
fn remove_available_keeps_unavailable_ids() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("failed.json");
    record_failures(&path, [1, 2, 3]);

    remove_available(&path, &HashSet::from([2]));
    let loaded = load(&path);

    assert_eq!(loaded.beatmapset_ids, vec![1, 3]);
}

#[test]
fn path_from_base_uses_osu_collect_data_file() {
    let path = failed_maps_path_from_base("/tmp/data".into());

    assert!(path.ends_with("osu-collect/failed-beatmapsets.json"));
}
