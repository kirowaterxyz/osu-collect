//! Configuration types for the downloader

use std::time::Duration;

#[derive(Debug, Clone)]
pub(crate) struct DownloadConfig {
    pub(crate) concurrent_downloads: usize,
    pub(crate) verify_archives: bool,
    pub(crate) progress_timeout: Duration,
    pub(crate) user_agent: String,
    pub(crate) max_retries: u32,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            concurrent_downloads: 4,
            verify_archives: true,
            progress_timeout: Duration::from_secs(30),
            user_agent: format!("osu-downloader/{}", env!("CARGO_PKG_VERSION")),
            max_retries: 3,
        }
    }
}
