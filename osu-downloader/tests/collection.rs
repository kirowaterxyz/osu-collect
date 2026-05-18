use super::{Beatmapset, Collection, Uploader, parse_collection_id_from_url};

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
fn parses_collection_id_from_url() {
    assert_eq!(
        parse_collection_id_from_url("https://osucollector.com/collections/12345").unwrap(),
        12345
    );
    assert_eq!(
        parse_collection_id_from_url("https://osucollector.com/collections/12345/").unwrap(),
        12345
    );
    assert!(parse_collection_id_from_url("invalid").is_err());
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
