//! Collection API support (feature: collection)
//!
//! This module provides functionality for fetching collections from osucollector.com
//! and creating osu! collection.db files.

use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

/// osu! database version for collection.db files
const OSU_DB_VERSION: u32 = 20210528;

/// Collection metadata from osucollector.com
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Collection {
    /// Collection ID
    pub id: u32,
    /// Collection name
    pub name: String,
    /// Collection description
    #[serde(default)]
    pub description: Option<String>,
    /// Uploader information
    pub uploader: Uploader,
    /// Beatmapsets in this collection
    pub beatmapsets: Vec<Beatmapset>,
    /// Number of favourites
    #[serde(default)]
    pub favourites: u32,
}

impl Collection {
    /// Get all beatmapset IDs in this collection
    pub fn beatmapset_ids(&self) -> Vec<u32> {
        self.beatmapsets.iter().map(|b| b.id).collect()
    }

    /// Get total number of beatmaps (not beatmapsets)
    pub fn beatmap_count(&self) -> usize {
        self.beatmapsets.iter().map(|b| b.beatmaps.len()).sum()
    }

    /// Download all beatmapsets in this collection
    ///
    /// # Arguments
    ///
    /// * `downloader` - The downloader instance to use
    /// * `output_dir` - Directory to save the beatmapsets
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use osu_downloader::{Downloader, collection::CollectionClient};
    /// # use futures_util::StreamExt;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let downloader = Downloader::builder().default_mirrors().build()?;
    /// let client = CollectionClient::new()?;
    /// let collection = client.fetch(12345).await?;
    ///
    /// let mut session = collection.download(&downloader, "./downloads");
    /// let mut events = session.events();
    /// while let Some(_event) = events.next().await {
    ///     // handle event
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn download(
        &self,
        downloader: &crate::Downloader,
        output_dir: impl AsRef<Path>,
    ) -> crate::DownloadSession {
        downloader.download_many(self.beatmapset_ids(), output_dir)
    }

    /// Write this collection to an osu! collection.db file
    ///
    /// Creates a collection.db file that can be imported into osu!
    pub fn write_db(&self, output_path: &Path) -> Result<()> {
        use osu_db::collection::{Collection as DbCollection, CollectionList};

        let beatmap_hashes: Vec<Option<String>> = self
            .beatmapsets
            .iter()
            .flat_map(|beatmapset| {
                beatmapset
                    .beatmaps
                    .iter()
                    .map(|beatmap| Some(beatmap.checksum.clone()))
            })
            .collect();

        let db_collection = DbCollection {
            name: Some(self.name.clone()),
            beatmap_hashes,
        };

        let collection_list = CollectionList {
            version: OSU_DB_VERSION,
            collections: vec![db_collection],
        };

        collection_list
            .to_file(output_path)
            .map_err(|e| Error::collection(format!("Failed to write collection.db: {}", e)))?;

        Ok(())
    }
}

/// Uploader information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Uploader {
    /// Uploader user ID
    pub id: u32,
    /// Uploader username
    pub username: String,
}

/// Beatmapset in a collection
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmapset {
    /// Beatmapset ID
    pub id: u32,
    /// Individual beatmaps in this set
    #[serde(default)]
    pub beatmaps: Vec<Beatmap>,
}

/// Individual beatmap
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmap {
    /// Beatmap ID
    pub id: u32,
    /// MD5 checksum
    pub checksum: String,
}

/// Client for fetching collections from osucollector.com
pub struct CollectionClient {
    client: reqwest::Client,
}

impl CollectionClient {
    /// Create a new collection client
    pub fn new() -> Result<Self> {
        let client = crate::http::create_api_client()?;
        Ok(Self { client })
    }

    /// Fetch a collection by ID
    ///
    /// # Arguments
    ///
    /// * `collection_id` - The collection ID from osucollector.com
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use osu_downloader::collection::CollectionClient;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let client = CollectionClient::new()?;
    /// let collection = client.fetch(12345).await?;
    ///
    /// println!("Collection: {}", collection.name);
    /// println!("Beatmapsets: {}", collection.beatmapsets.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn fetch(&self, collection_id: u32) -> Result<Collection> {
        let url = format!("https://osucollector.com/api/collections/{collection_id}");
        let response = self.client.get(url).send().await?;

        if !response.status().is_success() {
            return Err(Error::collection(format!(
                "API returned status {}",
                response.status()
            )));
        }

        Ok(response.json().await?)
    }

    /// Fetch a collection by URL
    ///
    /// Parses the collection ID from the URL and fetches it.
    pub async fn fetch_by_url(&self, url: &str) -> Result<Collection> {
        let collection_id = parse_collection_id_from_url(url)?;
        self.fetch(collection_id).await
    }
}

fn parse_collection_id_from_url(url: &str) -> Result<u32> {
    // URL format: https://osucollector.com/collections/12345
    url.split('/')
        .next_back()
        .and_then(|s| s.parse::<u32>().ok())
        .ok_or_else(|| Error::collection("Invalid collection URL"))
}

#[cfg(test)]
#[path = "../tests/collection.rs"]
mod tests;
