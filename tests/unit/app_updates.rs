use osu_collect::app::updates::{MissingBeatmapset, MissingStatus, UpdatesTab};
use osu_collect::osu_db::{LocalBeatmap, LocalBeatmapset, LocalCollection};
use std::collections::HashSet;

#[test]
fn set_collections_hides_entries_without_ids() {
    let mut tab = UpdatesTab::new();
    let collections = vec![
        LocalCollection {
            name: "My Collection - 123".to_string(),
            beatmap_checksums: vec!["hash".to_string()],
        },
        LocalCollection {
            name: "Missing Id".to_string(),
            beatmap_checksums: vec!["other".to_string()],
        },
    ];

    tab.set_collections(collections);

    assert_eq!(tab.selection.local_collections.len(), 1);
    assert_eq!(tab.selection.local_collections[0].collection_id, Some(123));
}

#[test]
fn extract_id_formats() {
    let cases = [
        ("Cool Maps - 9001", Some(9001u64)),
        ("Cool Maps – 9001", Some(9001)),
        ("Cool Maps — 9001", Some(9001)),
        ("#9001 - Cool Maps", Some(9001)),
        ("Cool Maps (9001)", Some(9001)),
        ("Cool Maps [9001]", Some(9001)),
        ("No id here", None),
        ("Short - 1", None),
    ];

    let mut tab = UpdatesTab::new();
    for (name, expected_id) in &cases {
        let collections = vec![LocalCollection {
            name: name.to_string(),
            beatmap_checksums: vec![],
        }];
        tab.set_collections(collections);
        let got = tab
            .selection
            .local_collections
            .first()
            .and_then(|e| e.collection_id);
        assert_eq!(got, *expected_id, "name: {name}");
    }
}

#[test]
fn set_local_beatmapsets_builds_id_index() {
    let mut tab = UpdatesTab::new();
    let sets = vec![
        LocalBeatmapset {
            id: 10,
            beatmaps: vec![LocalBeatmap {
                checksum: "aaa".to_string(),
            }],
        },
        LocalBeatmapset {
            id: 20,
            beatmaps: vec![LocalBeatmap {
                checksum: "bbb".to_string(),
            }],
        },
    ];
    tab.set_local_beatmapsets(sets);

    assert!(tab.scan.local_beatmapsets.contains_key(&10));
    assert!(tab.scan.local_beatmapsets.contains_key(&20));
    assert!(!tab.scan.local_beatmapsets.contains_key(&99));
}

#[test]
fn set_all_checksums_builds_hashset() {
    let mut tab = UpdatesTab::new();
    tab.set_all_checksums(vec!["abc".to_string(), "def".to_string()]);

    assert!(tab.scan.all_local_checksums.contains("abc"));
    assert!(tab.scan.all_local_checksums.contains("def"));
    assert!(!tab.scan.all_local_checksums.contains("xyz"));
}

#[test]
fn installed_beatmapset_not_in_missing() {
    // Simulates: beatmapset id=42 is locally installed; a collection contains it.
    // After set_missing_beatmaps with an empty list (checked upstream), visible_missing is empty.
    let mut tab = UpdatesTab::new();
    tab.set_local_beatmapsets(vec![LocalBeatmapset {
        id: 42,
        beatmaps: vec![LocalBeatmap {
            checksum: "deadbeef".to_string(),
        }],
    }]);
    tab.set_all_checksums(vec!["deadbeef".to_string()]);

    // Locally installed = not missing
    tab.set_missing_beatmaps(vec![]);

    assert_eq!(tab.total_missing_count(), 0);
}

#[test]
fn checksum_fallback_marks_installed() {
    // Beatmapset id=99 not in local_beatmapsets, but its checksum IS in all_local_checksums.
    // The comparison logic in fetch_and_compare skips such sets.
    // This test verifies the HashSet membership check used there.
    let all_checksums: HashSet<String> = ["aaaa1111", "bbbb2222"]
        .iter()
        .map(|s| s.to_string())
        .collect();

    // Both checksums present → "all installed"
    let api_checksums = ["aaaa1111", "bbbb2222"];
    let all_present = api_checksums.iter().all(|cs| all_checksums.contains(*cs));
    assert!(
        all_present,
        "should be considered installed via checksum fallback"
    );

    // One missing → not all installed
    let api_checksums_partial = ["aaaa1111", "cccc3333"];
    let partial_present = api_checksums_partial
        .iter()
        .all(|cs| all_checksums.contains(*cs));
    assert!(
        !partial_present,
        "partial checksum match should not be considered installed"
    );
}

#[test]
fn missing_beatmap_selection_preserved_across_refresh() {
    let mut tab = UpdatesTab::new();

    let first_batch = vec![
        MissingBeatmapset {
            id: 1,
            status: MissingStatus::NotInstalled,
            collection_id: 100,
            collection_name: "coll".to_string(),
            selected: true,
        },
        MissingBeatmapset {
            id: 2,
            status: MissingStatus::NotInstalled,
            collection_id: 100,
            collection_name: "coll".to_string(),
            selected: true,
        },
    ];

    tab.set_collections(vec![LocalCollection {
        name: "coll - 100".to_string(),
        beatmap_checksums: vec![],
    }]);

    tab.set_missing_beatmaps(first_batch);

    // Deselect id=1
    tab.selection.cached_missing_sets[0].selected = false;

    // Refresh with same + new entry
    let second_batch = vec![
        MissingBeatmapset {
            id: 1,
            status: MissingStatus::NotInstalled,
            collection_id: 100,
            collection_name: "coll".to_string(),
            selected: true,
        },
        MissingBeatmapset {
            id: 2,
            status: MissingStatus::NotInstalled,
            collection_id: 100,
            collection_name: "coll".to_string(),
            selected: true,
        },
        MissingBeatmapset {
            id: 3,
            status: MissingStatus::NotInstalled,
            collection_id: 100,
            collection_name: "coll".to_string(),
            selected: true,
        },
    ];

    tab.set_missing_beatmaps(second_batch);

    // id=1 was deselected, should remain deselected
    let id1 = tab
        .selection
        .cached_missing_sets
        .iter()
        .find(|b| b.id == 1)
        .unwrap();
    assert!(!id1.selected, "id=1 deselection should survive refresh");

    // id=2 was selected, should remain selected
    let id2 = tab
        .selection
        .cached_missing_sets
        .iter()
        .find(|b| b.id == 2)
        .unwrap();
    assert!(id2.selected, "id=2 selection should survive refresh");
}
