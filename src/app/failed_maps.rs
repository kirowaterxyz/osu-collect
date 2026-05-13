use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    env, fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};
use tracing::{debug, warn};

pub const FAILED_MAPS_FILE: &str = "failed-beatmapsets.json";
pub const FAILED_MAPS_ENV_PATH: &str = "OSU_COLLECT_FAILED_MAPS";
const SCHEMA_VERSION: u32 = 1;
static SAVE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedMapsFile {
    pub schema_version: u32,
    #[serde(default)]
    pub beatmapset_ids: Vec<u32>,
}

impl FailedMapsFile {
    pub fn ids(&self) -> HashSet<u32> {
        self.beatmapset_ids.iter().copied().collect()
    }

    pub fn insert_all(&mut self, ids: impl IntoIterator<Item = u32>) {
        self.beatmapset_ids.extend(ids);
        normalize(&mut self.beatmapset_ids);
    }

    pub fn remove_all(&mut self, ids: &HashSet<u32>) {
        self.beatmapset_ids.retain(|id| !ids.contains(id));
    }
}

pub fn failed_maps_path() -> Option<PathBuf> {
    if let Ok(custom) = env::var(FAILED_MAPS_ENV_PATH) {
        let trimmed = custom.trim();
        if !trimmed.is_empty() {
            return Some(PathBuf::from(trimmed));
        }
    }

    platform_data_dir().map(failed_maps_path_from_base)
}

pub fn failed_maps_path_from_base(base: PathBuf) -> PathBuf {
    base.join("osu-collect").join(FAILED_MAPS_FILE)
}

#[cfg(windows)]
fn platform_data_dir() -> Option<PathBuf> {
    dirs::data_dir()
}

#[cfg(not(windows))]
fn platform_data_dir() -> Option<PathBuf> {
    dirs::data_local_dir()
}

pub fn load(path: &Path) -> FailedMapsFile {
    match fs::read_to_string(path) {
        Err(_) => {
            debug!(path = %path.display(), "no failed maps file found");
            FailedMapsFile {
                schema_version: SCHEMA_VERSION,
                ..Default::default()
            }
        }
        Ok(contents) => match serde_json::from_str::<FailedMapsFile>(&contents) {
            Ok(mut failed_maps) => {
                failed_maps.schema_version = SCHEMA_VERSION;
                normalize(&mut failed_maps.beatmapset_ids);
                failed_maps
            }
            Err(err) => {
                warn!(path = %path.display(), error = %err, "failed to parse failed maps file");
                FailedMapsFile {
                    schema_version: SCHEMA_VERSION,
                    ..Default::default()
                }
            }
        },
    }
}

pub fn save(failed_maps: &FailedMapsFile, path: &Path) {
    let Ok(_guard) = SAVE_LOCK.lock() else {
        warn!(path = %path.display(), "failed to lock failed maps save");
        return;
    };

    let mut failed_maps = failed_maps.clone();
    failed_maps.schema_version = SCHEMA_VERSION;
    normalize(&mut failed_maps.beatmapset_ids);

    let contents = match serde_json::to_string_pretty(&failed_maps) {
        Ok(contents) => contents,
        Err(err) => {
            warn!(error = %err, "failed to serialize failed maps");
            return;
        }
    };

    if let Some(parent) = path.parent()
        && let Err(err) = fs::create_dir_all(parent)
    {
        warn!(path = %parent.display(), error = %err, "failed to create failed maps directory");
        return;
    }

    let tmp = path.with_extension("json.tmp");
    let write_result = (|| {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(contents.as_bytes())?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&tmp, path)?;
        Ok::<_, std::io::Error>(())
    })();

    if let Err(err) = write_result {
        warn!(path = %path.display(), error = %err, "failed to save failed maps");
        let _ = fs::remove_file(&tmp);
    } else {
        debug!(path = %path.display(), "saved failed maps");
    }
}

pub fn record_failures(path: &Path, ids: impl IntoIterator<Item = u32>) {
    let mut failed_maps = load(path);
    failed_maps.insert_all(ids);
    save(&failed_maps, path);
}

pub fn remove_available(path: &Path, available: &HashSet<u32>) {
    let mut failed_maps = load(path);
    failed_maps.remove_all(available);
    save(&failed_maps, path);
}

fn normalize(ids: &mut Vec<u32>) {
    ids.sort_unstable();
    ids.dedup();
}
