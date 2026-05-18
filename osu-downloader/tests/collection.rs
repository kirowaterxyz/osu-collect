#[cfg(feature = "collection")]
mod tests {
    use osu_downloader::__test_exports::parse_collection_id_from_url;
    use osu_downloader::collection::{Beatmapset, Collection, Uploader};

    #[test]
    fn test_parse_collection_id() {
        assert_eq!(
            parse_collection_id_from_url("https://osucollector.com/collections/12345").unwrap(),
            12345
        );
        assert!(parse_collection_id_from_url("invalid").is_err());
    }

    #[test]
    fn test_beatmapset_ids() {
        let collection = Collection {
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
        };

        assert_eq!(collection.beatmapset_ids(), vec![100, 200]);
    }
}
