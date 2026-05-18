//! osucollector.com client and `collection.db` writer.
//!
//! Behind the `collection` feature.

use crate::http;
use osu_db::collection::{Collection as DbCollection, CollectionList};
use serde::{Deserialize, Serialize};
use std::{fmt, io, path::Path, time::Duration};

const OSU_DB_VERSION: u32 = 20150203;
const COLLECTOR_API_BASE: &str = "https://osucollector.com/api/collections";

/// Collection metadata from osucollector.com.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Collection {
    /// Collection ID.
    pub id: u32,
    /// Collection name.
    pub name: String,
    /// Collection description.
    #[serde(default)]
    pub description: Option<String>,
    /// Uploader information.
    pub uploader: Uploader,
    /// Beatmapsets in this collection.
    pub beatmapsets: Vec<Beatmapset>,
    /// Number of favourites.
    #[serde(default)]
    pub favourites: u32,
}

/// Uploader information.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Uploader {
    /// Uploader user ID.
    pub id: u32,
    /// Uploader username.
    pub username: String,
}

/// Beatmapset in a collection.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmapset {
    /// Beatmapset ID.
    pub id: u32,
    /// Individual beatmaps in this set.
    #[serde(default)]
    pub beatmaps: Vec<Beatmap>,
}

/// Individual beatmap.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Beatmap {
    /// Beatmap ID.
    pub id: u32,
    /// MD5 checksum.
    pub checksum: String,
}

impl Collection {
    /// Beatmapset IDs in this collection (preserving order, deduplicated).
    pub fn beatmapset_ids(&self) -> Vec<u32> {
        let mut seen = std::collections::HashSet::new();
        self.beatmapsets
            .iter()
            .map(|b| b.id)
            .filter(|id| seen.insert(*id))
            .collect()
    }

    /// Total number of beatmaps (not beatmapsets).
    pub fn beatmap_count(&self) -> usize {
        self.beatmapsets.iter().map(|b| b.beatmaps.len()).sum()
    }

    /// Default folder name for this collection: `<sanitized-name>-<id>`.
    pub fn folder_name(&self) -> String {
        format!("{}-{}", sanitize_collection_name(&self.name), self.id)
    }

    /// Write this collection to `<output_path>` as `collection.db`.
    ///
    /// Uses `<name>-<id>` as the entry name.
    pub fn write_db(&self, output_path: &Path) -> io::Result<()> {
        let name = format!("{}-{}", self.name, self.id);
        self.write_db_as(&name, output_path)
    }

    /// Write this collection to `<output_path>` with a custom entry name.
    pub fn write_db_as(&self, name: &str, output_path: &Path) -> io::Result<()> {
        write_collections_db(
            &[CollectionDbEntry {
                name: name.to_string(),
                beatmap_hashes: collection_hashes(self),
            }],
            output_path,
        )
    }
}

/// Named set of beatmap hashes for [`write_collections_db`].
#[derive(Debug, Clone)]
pub struct CollectionDbEntry {
    /// Collection entry name as it will appear in osu!.
    pub name: String,
    /// Beatmap MD5 hashes. Duplicates within an entry are dropped on write.
    pub beatmap_hashes: Vec<String>,
}

/// Write one or more named collection entries to `<output_path>` in osu! `collection.db` format.
pub fn write_collections_db(entries: &[CollectionDbEntry], output_path: &Path) -> io::Result<()> {
    let collections = entries
        .iter()
        .map(|entry| {
            let mut seen = std::collections::HashSet::new();
            DbCollection {
                name: Some(entry.name.clone()),
                beatmap_hashes: entry
                    .beatmap_hashes
                    .iter()
                    .filter(|hash| seen.insert((*hash).clone()))
                    .cloned()
                    .map(Some)
                    .collect(),
            }
        })
        .collect();

    CollectionList {
        version: OSU_DB_VERSION,
        collections,
    }
    .to_file(output_path)
    .map_err(|err| io::Error::other(err.to_string()))
}

fn collection_hashes(collection: &Collection) -> Vec<String> {
    collection
        .beatmapsets
        .iter()
        .flat_map(|beatmapset| {
            beatmapset
                .beatmaps
                .iter()
                .map(|beatmap| beatmap.checksum.clone())
        })
        .collect()
}

fn sanitize_collection_name(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        out.push(match c {
            '/' | '\\' | '\0' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            other => other,
        });
    }

    let trimmed = out.trim();
    if trimmed.is_empty() {
        "collection".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Result type for collection client operations.
pub type CollectionResult<T> = std::result::Result<T, CollectionError>;

/// Errors returned by [`CollectionClient`] operations.
#[derive(Debug)]
pub enum CollectionError {
    /// Transport-level failure (connect/timeout/decode/etc.).
    Network(reqwest::Error),
    /// The collection ID was not found (HTTP 404).
    NotFound,
    /// The server returned HTTP 429. `retry_after` is the `Retry-After` header value if present.
    RateLimited {
        /// Cooldown the server asked the client to wait.
        retry_after: Option<Duration>,
    },
    /// Server returned an unsuccessful status code (other than 404/429).
    Status(reqwest::StatusCode),
    /// URL passed to [`CollectionClient::fetch_by_url`] could not be parsed.
    InvalidUrl(String),
    /// Failed to decode the collection JSON.
    Parse(serde_json::Error),
}

impl fmt::Display for CollectionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CollectionError::Network(err) => write!(f, "network error: {err}"),
            CollectionError::NotFound => f.write_str("collection not found"),
            CollectionError::RateLimited { retry_after } => match retry_after {
                Some(duration) => write!(f, "rate limited (retry after {}s)", duration.as_secs()),
                None => f.write_str("rate limited"),
            },
            CollectionError::Status(status) => write!(f, "unexpected status {status}"),
            CollectionError::InvalidUrl(url) => write!(f, "invalid collection URL: {url}"),
            CollectionError::Parse(err) => write!(f, "failed to parse collection JSON: {err}"),
        }
    }
}

impl std::error::Error for CollectionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CollectionError::Network(err) => Some(err),
            CollectionError::Parse(err) => Some(err),
            _ => None,
        }
    }
}

/// Client for fetching collections from osucollector.com.
pub struct CollectionClient {
    client: reqwest::Client,
}

impl CollectionClient {
    /// New client backed by the library's default reqwest client.
    ///
    /// # Panics
    ///
    /// Panics if the underlying reqwest client builder fails — which only
    /// happens if the system's TLS backend cannot initialise.
    pub fn new() -> Self {
        Self::with_client(
            http::create_api_client().expect("failed to build default reqwest client"),
        )
    }

    /// New client reusing a caller-provided [`reqwest::Client`].
    pub fn with_client(client: reqwest::Client) -> Self {
        Self { client }
    }

    /// Fetch a collection by ID. Performs a single request — wrap in a retry
    /// loop on the caller side if you need to retry rate limits or transient errors.
    pub async fn fetch(&self, collection_id: u32) -> CollectionResult<Collection> {
        let url = format!("{COLLECTOR_API_BASE}/{collection_id}");
        let response = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(CollectionError::Network)?;

        let status = response.status();

        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse::<u64>().ok())
                .map(Duration::from_secs);
            return Err(CollectionError::RateLimited { retry_after });
        }

        if status == reqwest::StatusCode::NOT_FOUND {
            return Err(CollectionError::NotFound);
        }

        if !status.is_success() {
            return Err(CollectionError::Status(status));
        }

        let bytes = response.bytes().await.map_err(CollectionError::Network)?;
        serde_json::from_slice(&bytes).map_err(CollectionError::Parse)
    }

    /// Fetch a collection from a `https://osucollector.com/collections/<id>` URL.
    pub async fn fetch_by_url(&self, url: &str) -> CollectionResult<Collection> {
        let collection_id = parse_collection_id_from_url(url)?;
        self.fetch(collection_id).await
    }
}

impl Default for CollectionClient {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_collection_id_from_url(url: &str) -> CollectionResult<u32> {
    url.trim_end_matches('/')
        .rsplit('/')
        .next()
        .and_then(|tail| tail.parse::<u32>().ok())
        .ok_or_else(|| CollectionError::InvalidUrl(url.to_string()))
}

#[cfg(test)]
#[path = "../tests/collection.rs"]
mod tests;
