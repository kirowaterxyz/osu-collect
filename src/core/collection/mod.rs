pub mod api_client;

pub use api_client::{CollectionService, HttpCollectionService};

pub use osu_downloader::collection::{
    Beatmap, Beatmapset, Collection, CollectionDbEntry, Uploader, write_collections_db,
};

/// Generate the folder name that will host the downloaded beatmaps.
pub fn folder_name(collection: &Collection) -> String {
    collection.folder_name()
}

#[cfg(test)]
pub(crate) fn test_collection(id: u32, beatmapsets: Vec<Beatmapset>) -> Collection {
    Collection {
        id,
        name: format!("collection-{id}"),
        description: None,
        uploader: Uploader {
            id: 0,
            username: String::new(),
        },
        beatmapsets,
        favourites: 0,
    }
}

#[cfg(test)]
pub(crate) fn test_beatmapset(id: u32, checksums: &[&str]) -> Beatmapset {
    Beatmapset {
        id,
        beatmaps: checksums
            .iter()
            .enumerate()
            .map(|(i, &checksum)| Beatmap {
                id: i as u32,
                checksum: checksum.to_string(),
            })
            .collect(),
    }
}
