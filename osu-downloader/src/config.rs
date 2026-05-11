//! Configuration types for the downloader

use std::time::Duration;

/// Internal configuration for the downloader
#[derive(Debug, Clone)]
pub(crate) struct DownloadConfig {
    /// Number of concurrent downloads
    pub concurrent_downloads: usize,
    /// Whether to verify ZIP archives
    pub verify_archives: bool,
    /// Timeout for download progress (if no bytes received)
    pub progress_timeout: Duration,
    /// User agent string
    pub user_agent: String,
    /// maximum retry attempts per mirror for transient failures
    pub max_retries: u32,
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
