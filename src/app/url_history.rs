use crate::config::constants::CONFIG_SUBDIR;
use serde::{Deserialize, Serialize};
use std::{fs, path::PathBuf};
use tracing::{debug, warn};

pub const URL_HISTORY_FILE: &str = "url-history.json";
const SCHEMA_VERSION: u32 = 1;
const MAX_ENTRIES: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UrlHistoryEntry {
    pub url: String,
    pub name: String,
    pub count: usize,
    pub last_used: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UrlHistoryFile {
    pub schema_version: u32,
    #[serde(default)]
    pub entries: Vec<UrlHistoryEntry>,
}

/// Load url history from `${config_dir}/osu-collect/url-history.json`.
/// Returns an empty history on absent file or parse error.
pub fn load() -> UrlHistoryFile {
    let Some(path) = history_path() else {
        return empty();
    };
    match fs::read_to_string(&path) {
        Err(_) => {
            debug!(path = %path.display(), "no url history file found");
            empty()
        }
        Ok(contents) => match serde_json::from_str::<UrlHistoryFile>(&contents) {
            Ok(mut file) => {
                file.schema_version = SCHEMA_VERSION;
                file.entries.truncate(MAX_ENTRIES);
                file
            }
            Err(err) => {
                warn!(path = %path.display(), error = %err, "failed to parse url history");
                empty()
            }
        },
    }
}

/// Write history atomically to `${config_dir}/osu-collect/url-history.json`.
pub fn save(history: &UrlHistoryFile) {
    let Some(path) = history_path() else {
        return;
    };
    let mut to_save = history.clone();
    to_save.schema_version = SCHEMA_VERSION;
    to_save.entries.truncate(MAX_ENTRIES);

    let contents = match serde_json::to_string_pretty(&to_save) {
        Ok(c) => c,
        Err(err) => {
            warn!(error = %err, "failed to serialize url history");
            return;
        }
    };

    if let Err(err) = super::write_atomic(&path, "json.tmp", &contents) {
        warn!(path = %path.display(), error = %err, "failed to save url history");
    } else {
        debug!(path = %path.display(), "saved url history");
    }
}

/// Upsert `entry` to the front of `history`, dedupe by URL, cap at 10.
pub fn push(history: &mut UrlHistoryFile, entry: UrlHistoryEntry) {
    history.entries.retain(|e| e.url != entry.url);
    history.entries.insert(0, entry);
    history.entries.truncate(MAX_ENTRIES);
}

pub fn history_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join(CONFIG_SUBDIR).join(URL_HISTORY_FILE))
}

fn empty() -> UrlHistoryFile {
    UrlHistoryFile {
        schema_version: SCHEMA_VERSION,
        entries: Vec::new(),
    }
}

#[cfg(test)]
#[path = "../../tests/unit/url_history.rs"]
mod tests;
