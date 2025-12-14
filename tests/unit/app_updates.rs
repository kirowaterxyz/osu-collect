use osu_collect::app::updates::UpdatesTab;
use osu_collect::osu_db::LocalCollection;

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
