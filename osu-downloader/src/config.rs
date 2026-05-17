//! Configuration types for the downloader

use crate::validation::ArchiveValidation;
use std::time::Duration;

pub(crate) const TRANSIENT_RETRY_ATTEMPTS: u32 = 3;
pub(crate) const TRANSIENT_RETRY_BASE_DELAY: Duration = Duration::from_millis(500);
/// Delay between full network-retry passes when every mirror has exhausted transient errors.
pub(crate) const NETWORK_RETRY_BACKOFF: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub(crate) struct DownloadConfig {
    pub(crate) concurrent_downloads: usize,
    pub(crate) archive_validation: ArchiveValidation,
    pub(crate) progress_timeout: Duration,
    pub(crate) user_agent: String,
    pub(crate) network_retry_attempts: usize,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            concurrent_downloads: 4,
            archive_validation: ArchiveValidation::Magic,
            progress_timeout: Duration::from_secs(30),
            user_agent: format!("osu-downloader/{}", env!("CARGO_PKG_VERSION")),
            network_retry_attempts: 0,
        }
    }
}
