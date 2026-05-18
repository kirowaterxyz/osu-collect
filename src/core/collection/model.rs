use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Collection {
    pub id: u32,
    pub name: Box<str>,
    pub uploader: Uploader,
    pub beatmapsets: Vec<Beatmapset>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Uploader {
    pub id: u32,
    pub username: Box<str>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmapset {
    pub id: u32,
    #[serde(default)]
    pub beatmaps: Vec<Beatmap>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmap {
    pub id: u32,
    pub checksum: Box<str>,
}

#[doc(hidden)]
pub fn test_collection(id: u32, beatmapsets: Vec<Beatmapset>) -> Collection {
    Collection {
        id,
        name: format!("collection-{id}").into(),
        uploader: Uploader {
            id: 0,
            username: "".into(),
        },
        beatmapsets,
    }
}

#[doc(hidden)]
pub fn test_beatmapset(id: u32, checksums: &[&str]) -> Beatmapset {
    Beatmapset {
        id,
        beatmaps: checksums
            .iter()
            .enumerate()
            .map(|(i, &checksum)| Beatmap {
                id: i as u32,
                checksum: checksum.into(),
            })
            .collect(),
    }
}
