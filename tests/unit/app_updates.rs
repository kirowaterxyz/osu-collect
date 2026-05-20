use crate::app::{
    collection_state::CollectionStateFile,
    runtime::{
        FetchCompareSettings, collection_ids_for_scan, deleted_maps_for_scan,
        should_hide_failed_beatmapset,
    },
    updates::{MissingBeatmapset, MissingStatus, UpdatesTab},
};
use crate::osu_db::{LocalBeatmap, LocalBeatmapset, LocalCollection, Md5};
use std::collections::HashSet;

fn test_md5(seed: u8) -> Md5 {
    let mut out = [0u8; 16];
    out[0] = seed;
    out
}

fn missing(id: u32, selected: bool, previously_deleted: bool) -> MissingBeatmapset {
    MissingBeatmapset {
        id,
        status: MissingStatus::NotInstalled,
        collection_id: 100,
        collection_name: "coll".to_string(),
        selected,
        previously_deleted,
    }
}

#[test]
fn set_collections_hides_entries_without_ids() {
    let mut tab = UpdatesTab::new();
    let collections = vec![
        LocalCollection {
            name: "My Collection - 123".to_string(),
            beatmap_checksums: vec![test_md5(1)],
        },
        LocalCollection {
            name: "Missing Id".to_string(),
            beatmap_checksums: vec![test_md5(2)],
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
fn collection_ids_for_scan_uses_selected_ids_only() {
    assert_eq!(collection_ids_for_scan(vec![1, 3]), vec![1, 3]);
}

#[test]
fn collection_ids_for_scan_skips_ids_outside_u32() {
    assert_eq!(
        collection_ids_for_scan(vec![42, u64::from(u32::MAX) + 1]),
        vec![42]
    );
}

#[test]
fn deleted_maps_for_scan_only_uses_selected_collections() {
    let mut state = CollectionStateFile::default();
    state.update(10, vec![1, 2, 3], vec![1, 3, 999], vec![]);
    state.update(20, vec![4, 5], vec![4, 5], vec![]);

    let deleted = deleted_maps_for_scan(&state, &[10]);

    assert_eq!(deleted.len(), 1);
    assert!(deleted[&10].contains(&1));
    assert!(deleted[&10].contains(&3));
    assert!(!deleted[&10].contains(&999));
    assert!(!deleted.contains_key(&20));
}

#[test]
fn set_local_beatmapsets_stores_sets() {
    let mut tab = UpdatesTab::new();
    let sets = vec![
        LocalBeatmapset {
            id: 10,
            beatmaps: vec![LocalBeatmap {
                checksum: test_md5(0xaa),
            }],
        },
        LocalBeatmapset {
            id: 20,
            beatmaps: vec![LocalBeatmap {
                checksum: test_md5(0xbb),
            }],
        },
    ];
    tab.set_local_beatmapsets(sets);

    let ids: Vec<u32> = tab.scan.local_beatmapsets.iter().map(|bs| bs.id).collect();
    assert!(ids.contains(&10));
    assert!(ids.contains(&20));
    assert!(!ids.contains(&99));
}

#[test]
fn set_all_checksums_builds_hashset() {
    let abc = test_md5(0xab);
    let def = test_md5(0xde);
    let xyz = test_md5(0xff);
    let mut tab = UpdatesTab::new();
    tab.set_all_checksums(vec![abc, def]);

    assert!(tab.scan.all_local_checksums.contains(&abc));
    assert!(tab.scan.all_local_checksums.contains(&def));
    assert!(!tab.scan.all_local_checksums.contains(&xyz));
}

#[test]
fn installed_beatmapset_not_in_missing() {
    // Simulates: beatmapset id=42 is locally installed; a collection contains it.
    // After set_missing_beatmaps with an empty list (checked upstream), visible_missing is empty.
    let cksum = test_md5(0xd0);
    let mut tab = UpdatesTab::new();
    tab.set_local_beatmapsets(vec![LocalBeatmapset {
        id: 42,
        beatmaps: vec![LocalBeatmap { checksum: cksum }],
    }]);
    tab.set_all_checksums(vec![cksum]);

    // Locally installed = not missing
    tab.set_missing_beatmaps(vec![]);

    assert_eq!(tab.total_missing_count(), 0);
}

#[test]
fn missing_beatmap_selection_preserved_across_refresh() {
    let mut tab = UpdatesTab::new();

    let first_batch = vec![missing(1, true, false), missing(2, true, false)];

    tab.set_collections(vec![LocalCollection {
        name: "coll - 100".to_string(),
        beatmap_checksums: vec![],
    }]);

    tab.set_missing_beatmaps(first_batch);

    // Deselect id=1
    tab.selection.cached_missing_sets[0].selected = false;

    // Refresh with same + new entry
    let second_batch = vec![
        missing(1, true, false),
        missing(2, true, false),
        missing(3, true, false),
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

#[test]
fn fetch_compare_settings_identifies_hidden_failed_ids() {
    let settings = FetchCompareSettings {
        hidden_failed_beatmapset_ids: HashSet::from([1234]),
    };

    assert!(should_hide_failed_beatmapset(&settings, 1234));
    assert!(!should_hide_failed_beatmapset(&settings, 5678));
}

#[test]
fn previously_deleted_items_are_deselected_by_default() {
    let mut tab = UpdatesTab::new();

    tab.set_collections(vec![LocalCollection {
        name: "coll - 100".to_string(),
        beatmap_checksums: vec![],
    }]);

    let batch = vec![missing(10, false, true), missing(20, true, false)];

    tab.set_missing_beatmaps(batch);

    let del = tab
        .selection
        .cached_missing_sets
        .iter()
        .find(|b| b.id == 10)
        .unwrap();
    assert!(!del.selected, "previously deleted must stay deselected");

    let new = tab
        .selection
        .cached_missing_sets
        .iter()
        .find(|b| b.id == 20)
        .unwrap();
    assert!(new.selected, "new item must be selected");
}

#[test]
fn previously_deleted_can_be_reselected_and_survives_refresh() {
    let mut tab = UpdatesTab::new();

    tab.set_collections(vec![LocalCollection {
        name: "coll - 100".to_string(),
        beatmap_checksums: vec![],
    }]);

    let first = vec![missing(10, false, true)];
    tab.set_missing_beatmaps(first);

    // User manually re-selects the previously deleted item
    tab.selection.cached_missing_sets[0].selected = true;

    // Second scan arrives: same item still marked previously_deleted
    let second = vec![missing(10, false, true)];
    tab.set_missing_beatmaps(second);

    let item = tab
        .selection
        .cached_missing_sets
        .iter()
        .find(|b| b.id == 10)
        .unwrap();
    assert!(item.selected, "user re-selection must survive refresh");
}
