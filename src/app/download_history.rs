use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::{debug, warn};

pub const DOWNLOAD_HISTORY_FILE: &str = "download-history.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DownloadHistoryEntry {
    pub collection_id: u32,
    pub name: String,
    pub completed_at: String,
    pub count: usize,
}

impl DownloadHistoryEntry {
    pub fn new(collection_id: u32, name: String, count: usize) -> Self {
        let completed_at = OffsetDateTime::now_utc()
            .format(&Rfc3339)
            .unwrap_or_else(|_| String::new());
        Self {
            collection_id,
            name,
            completed_at,
            count,
        }
    }
}

/// Load the download history from `path`. Returns an empty list on absent file
/// or parse error — never panics.
pub fn load(path: &std::path::Path) -> Vec<DownloadHistoryEntry> {
    match fs::read_to_string(path) {
        Err(_) => {
            debug!(path = %path.display(), "no download history file found");
            Vec::new()
        }
        Ok(contents) => match serde_json::from_str::<Vec<DownloadHistoryEntry>>(&contents) {
            Ok(entries) => entries,
            Err(err) => {
                warn!(path = %path.display(), error = %err, "failed to parse download history");
                Vec::new()
            }
        },
    }
}

/// Append `entry` to the history file at `path` atomically.
///
/// Reads the existing list, pushes the new entry, and writes back. Logs a
/// warning on any I/O or serialization failure — never panics.
pub fn append(path: &std::path::Path, entry: DownloadHistoryEntry) {
    let mut entries = load(path);
    entries.push(entry);

    let contents = match serde_json::to_string_pretty(&entries) {
        Ok(c) => c,
        Err(err) => {
            warn!(error = %err, "failed to serialize download history");
            return;
        }
    };

    if let Err(err) = super::write_atomic(path, "json.tmp", &contents) {
        warn!(path = %path.display(), error = %err, "failed to save download history");
    } else {
        debug!(path = %path.display(), "saved download history");
    }
}

pub fn history_path() -> Option<PathBuf> {
    platform_data_dir().map(|d| d.join("osu-collect").join(DOWNLOAD_HISTORY_FILE))
}

#[cfg(windows)]
fn platform_data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

#[cfg(not(windows))]
fn platform_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir()
}

#[cfg(test)]
#[path = "../../tests/unit/download_history.rs"]
mod tests;
