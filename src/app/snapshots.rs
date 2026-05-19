use crate::{
    core::collection::Beatmapset,
    osu_db::{LocalBeatmapset, LocalCollection, OsuClient},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};
use tracing::{debug, warn};

pub const SNAPSHOT_ENV_DIR: &str = "OSU_COLLECT_SNAPSHOT_DIR";
const SNAPSHOT_VERSION: u32 = 1;
static SAVE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionSnapshot {
    #[serde(default)]
    pub stable_hashes: Vec<String>,
    #[serde(default)]
    pub lazer_ids: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionSnapshotFile {
    pub collection_id: String,
    pub name: String,
    pub last_run_at: String,
    pub snapshot: CollectionSnapshot,
    pub version: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SnapshotDiff {
    pub manually_deleted: CollectionSnapshot,
    pub manually_added: CollectionSnapshot,
}

impl CollectionSnapshot {
    pub fn len(&self) -> usize {
        self.stable_hashes.len() + self.lazer_ids.len()
    }

    pub fn is_empty(&self) -> bool {
        self.stable_hashes.is_empty() && self.lazer_ids.is_empty()
    }
}

impl CollectionSnapshotFile {
    pub fn new(collection_id: u32, name: String, snapshot: CollectionSnapshot) -> Self {
        Self {
            collection_id: collection_id.to_string(),
            name,
            last_run_at: OffsetDateTime::now_utc()
                .format(&Rfc3339)
                .unwrap_or_else(|_| "1970-01-01t00:00:00z".to_string()),
            snapshot,
            version: SNAPSHOT_VERSION,
        }
    }
}

pub fn snapshots_dir() -> Option<PathBuf> {
    if let Ok(custom) = env::var(SNAPSHOT_ENV_DIR) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    platform_data_dir().map(snapshot_dir_from_base)
}

pub fn snapshot_dir_from_base(base: PathBuf) -> PathBuf {
    base.join("osu-collect").join("snapshots")
}

#[cfg(windows)]
fn platform_data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

#[cfg(not(windows))]
fn platform_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir()
}

pub fn snapshot_path(dir: &Path, collection_id: u32) -> PathBuf {
    dir.join(format!("collection-{collection_id}.json"))
}

pub fn load(path: &Path) -> Option<CollectionSnapshotFile> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            debug!(path = %path.display(), "no collection snapshot found");
            return None;
        }
        Err(err) => {
            warn!(path = %path.display(), error = %err, "failed to read collection snapshot");
            return None;
        }
    };

    let snapshot = match serde_json::from_str::<CollectionSnapshotFile>(&contents) {
        Ok(snapshot) => snapshot,
        Err(err) => {
            warn!(path = %path.display(), error = %err, "failed to parse collection snapshot");
            return None;
        }
    };

    if snapshot.version != SNAPSHOT_VERSION {
        warn!(
            path = %path.display(),
            version = snapshot.version,
            supported = SNAPSHOT_VERSION,
            "unsupported collection snapshot version"
        );
        return None;
    }

    Some(snapshot)
}

pub fn save(snapshot: &CollectionSnapshotFile, path: &Path) {
    let Ok(_guard) = SAVE_LOCK.lock() else {
        warn!(path = %path.display(), "failed to lock collection snapshot save");
        return;
    };

    let contents = match serde_json::to_string_pretty(snapshot) {
        Ok(contents) => contents,
        Err(err) => {
            warn!(error = %err, "failed to serialize collection snapshot");
            return;
        }
    };

    if let Err(err) = super::write_atomic(path, "json.tmp", &contents) {
        warn!(path = %path.display(), error = %err, "failed to save collection snapshot");
    } else {
        debug!(path = %path.display(), "saved collection snapshot");
    }
}

pub fn diff_snapshot(
    previous: Option<&CollectionSnapshot>,
    current: &CollectionSnapshot,
) -> SnapshotDiff {
    let Some(previous) = previous else {
        return SnapshotDiff::default();
    };

    SnapshotDiff {
        manually_deleted: CollectionSnapshot {
            stable_hashes: difference(&previous.stable_hashes, &current.stable_hashes),
            lazer_ids: difference(&previous.lazer_ids, &current.lazer_ids),
        },
        manually_added: CollectionSnapshot {
            stable_hashes: difference(&current.stable_hashes, &previous.stable_hashes),
            lazer_ids: difference(&current.lazer_ids, &previous.lazer_ids),
        },
    }
}

pub fn current_snapshots<'a>(
    client: OsuClient,
    collections: &[LocalCollection],
    beatmapsets: impl IntoIterator<Item = &'a LocalBeatmapset>,
    collection_id_for_name: impl Fn(&str) -> Option<u32>,
) -> HashMap<u32, CollectionSnapshotFile> {
    let checksum_index = checksum_beatmapset_index(beatmapsets);

    collections
        .iter()
        .filter_map(|collection| {
            let collection_id = collection_id_for_name(&collection.name)?;
            let snapshot = match client {
                OsuClient::Stable => CollectionSnapshot {
                    stable_hashes: sorted_unique(collection.beatmap_checksums.clone()),
                    lazer_ids: Vec::new(),
                },
                OsuClient::Lazer => CollectionSnapshot {
                    stable_hashes: Vec::new(),
                    lazer_ids: sorted_unique(
                        collection
                            .beatmap_checksums
                            .iter()
                            .filter_map(|checksum| checksum_index.get(checksum).copied())
                            .map(u64::from)
                            .collect(),
                    ),
                },
            };
            Some((
                collection_id,
                CollectionSnapshotFile::new(collection_id, collection.name.clone(), snapshot),
            ))
        })
        .collect()
}

pub fn in_deleted_snapshot(
    client: OsuClient,
    beatmapset: &Beatmapset,
    deleted: &CollectionSnapshot,
) -> bool {
    match client {
        OsuClient::Stable => {
            let deleted_hashes: HashSet<&str> =
                deleted.stable_hashes.iter().map(String::as_str).collect();
            beatmapset
                .beatmaps
                .iter()
                .map(|beatmap| beatmap.checksum.as_str())
                .any(|checksum| !checksum.is_empty() && deleted_hashes.contains(checksum))
        }
        OsuClient::Lazer => deleted.lazer_ids.contains(&u64::from(beatmapset.id)),
    }
}

fn checksum_beatmapset_index<'a>(
    beatmapsets: impl IntoIterator<Item = &'a LocalBeatmapset>,
) -> HashMap<String, u32> {
    let mut index = HashMap::new();
    for beatmapset in beatmapsets {
        for beatmap in &beatmapset.beatmaps {
            if !beatmap.checksum.is_empty() {
                index.insert(beatmap.checksum.clone(), beatmapset.id);
            }
        }
    }
    index
}

fn difference<T>(left: &[T], right: &[T]) -> Vec<T>
where
    T: Clone + Eq + std::hash::Hash + Ord,
{
    let right: HashSet<&T> = right.iter().collect();
    let mut values: Vec<T> = left
        .iter()
        .filter(|value| !right.contains(value))
        .cloned()
        .collect();
    values.sort_unstable();
    values.dedup();
    values
}

fn sorted_unique<T: Ord>(mut values: Vec<T>) -> Vec<T> {
    values.sort_unstable();
    values.dedup();
    values
}

#[cfg(test)]
#[path = "../../tests/unit/collection_snapshots.rs"]
mod tests;
