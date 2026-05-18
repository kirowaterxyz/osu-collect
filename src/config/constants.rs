use super::model::{LogFormat, LogLevel};
use osu_downloader::ArchiveValidation;
use std::time::Duration;

pub const KB: f64 = 1024.0;
pub const MB: f64 = KB * 1024.0;
pub const GB: f64 = MB * 1024.0;

pub const MAX_TRUNCATED_CHARS: usize = 80;

pub mod status {
    pub const RATE_LIMITED: &str = "rate limited";
    pub const ABORTED: &str = "aborted";
    pub const RECHECKING_PREFIX: &str = "rechecking";
    pub const DOWNLOADING: &str = "downloading";
}

pub const CONCURRENT_REQUESTS: usize = 50;
pub const DOWNLOAD_TIMEOUT_SECS: u64 = 60;
pub const DEFAULT_PROGRESS_WATCHDOG_SECS: u64 = 120;
pub const COLLECTION_FETCH_TIMEOUT_SECS: u64 = 30;

pub fn default_threads() -> u8 {
    std::thread::available_parallelism()
        .map(|n| (n.get() as u8).min(50))
        .unwrap_or(1)
}

/// Number of transient-error retry attempts per mirror inside a single download attempt.
pub const TRANSIENT_RETRY_ATTEMPTS: u8 = 3;
/// Base delay between transient retries (doubles each attempt).
pub const TRANSIENT_RETRY_BASE_DELAY: Duration = Duration::from_millis(800);
/// Maximum number of additional passes through the mirror pool after every mirror has
/// exhausted its transient retries. The library waits 5 seconds between passes
/// (cancellable). Beyond this cap the beatmapset is reported as `BeatmapsetNetworkError`.
pub const NETWORK_RETRY_CAP: u32 = 1000;

pub const CONFIG_SUBDIR: &str = "osu-collect";
pub const CONFIG_FILE: &str = "config.toml";
pub const CONFIG_ENV_PATH: &str = "OSU_COLLECT_CONFIG";

pub const RELEASES_URL: &str = "https://api.github.com/repos/uwuclxdy/osu-collect/releases/latest";
pub const AUTO_UPDATE_TIMEOUT: Duration = Duration::from_secs(60);

pub const SPEED_UPDATE_INTERVAL: Duration = Duration::from_millis(250);
pub const SPEED_STALE_AFTER: Duration = Duration::from_secs(1);
pub const COMPLETION_PREFIXES: [&str; 4] = ["Done", "Skipped", "Failed", "Accepted"];
pub const MAX_LOG_LINES: usize = 5;

pub const VALIDATION_CACHE_LIMIT: usize = 4096;

pub const LOG_LEVELS: [LogLevel; 5] = [
    LogLevel::Error,
    LogLevel::Warn,
    LogLevel::Info,
    LogLevel::Debug,
    LogLevel::Trace,
];

pub const LOG_FORMATS: [LogFormat; 2] = [LogFormat::Compact, LogFormat::Pretty];

pub const ARCHIVE_VALIDATIONS: [ArchiveValidation; 3] = [
    ArchiveValidation::Off,
    ArchiveValidation::Magic,
    ArchiveValidation::Eocd,
];

pub const HOME_TAB_INDEX: usize = 0;
pub const UPDATES_TAB_INDEX: usize = 1;
pub const CONFIG_TAB_INDEX: usize = 2;
pub const STATIC_TABS: usize = 3;

pub const NEKOHA_API_BASE: &str = "https://mirror.nekoha.moe/api4";

pub const LOW_SPACE_THRESHOLD_BYTES: u64 = 1024 * 1024 * 1024;

pub const DIRECTORY_LOCK_FILE: &str = ".osu-collect.lock";

pub const API_MAX_RETRIES: u8 = 3;
