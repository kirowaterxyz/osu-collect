//! Event types for download progress and status updates

use crate::{DownloadError, MirrorKind};
use std::time::Duration;

/// Events emitted during download session
#[derive(Debug, Clone)]
pub enum DownloadEvent {
    /// Download session has started
    SessionStarted {
        /// Total number of beatmapsets to download
        total_beatmapsets: usize,
    },

    /// A beatmapset download has started
    BeatmapsetStarted {
        /// Beatmapset ID
        beatmapset_id: u32,
        /// Mirror being used for download
        mirror: MirrorKind,
    },

    /// Download progress update for a beatmapset
    Progress {
        /// Beatmapset ID
        beatmapset_id: u32,
        /// Number of bytes downloaded so far
        downloaded_bytes: u64,
        /// Total size in bytes (None if unknown)
        total_bytes: Option<u64>,
        /// Download speed in bytes per second
        speed_bps: u64,
    },

    /// A beatmapset was downloaded successfully
    BeatmapsetCompleted {
        /// Beatmapset ID
        beatmapset_id: u32,
        /// Downloaded filename
        filename: String,
        /// File size in bytes
        size_bytes: u64,
        /// MD5 hash (if computed)
        md5_hash: Option<String>,
        /// Mirror that was used
        mirror_used: MirrorKind,
    },

    /// A beatmapset download failed
    BeatmapsetFailed {
        /// Beatmapset ID
        beatmapset_id: u32,
        /// Error that occurred
        error: DownloadError,
        /// Number of retries attempted
        retry_count: u32,
    },

    /// A beatmapset was skipped
    BeatmapsetSkipped {
        /// Beatmapset ID
        beatmapset_id: u32,
        /// Reason for skipping
        reason: SkipReason,
    },

    /// Download session has completed
    SessionCompleted {
        /// Download summary statistics
        summary: DownloadSummary,
    },
}

/// Reason a beatmapset was skipped
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Beatmapset already exists at destination
    AlreadyExists,
    /// Beatmapset is not available on any configured mirror
    UnavailableOnMirrors,
    /// Invalid beatmapset ID
    InvalidBeatmapsetId,
}

/// Summary of a completed download session
#[derive(Debug, Clone)]
pub struct DownloadSummary {
    /// Total number of beatmapsets in session
    pub total: usize,
    /// IDs of successfully downloaded beatmapsets
    pub downloaded: Vec<u32>,
    /// IDs and reasons for skipped beatmapsets
    pub skipped: Vec<(u32, SkipReason)>,
    /// IDs and error messages for failed beatmapsets
    pub failed: Vec<(u32, String)>,
    /// Total bytes downloaded
    pub total_bytes: u64,
    /// Duration of the download session
    pub duration: Duration,
}

impl DownloadSummary {
    pub(crate) fn new(total: usize) -> Self {
        Self {
            total,
            downloaded: Vec::new(),
            skipped: Vec::new(),
            failed: Vec::new(),
            total_bytes: 0,
            duration: Duration::ZERO,
        }
    }

    /// Get the number of successfully downloaded beatmapsets
    pub fn downloaded_count(&self) -> usize {
        self.downloaded.len()
    }

    /// Get the number of skipped beatmapsets
    pub fn skipped_count(&self) -> usize {
        self.skipped.len()
    }

    /// Get the number of failed beatmapsets
    pub fn failed_count(&self) -> usize {
        self.failed.len()
    }

    /// Check if all downloads were successful
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty() && self.skipped.is_empty()
    }

    /// Get success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total == 0 {
            return 1.0;
        }
        self.downloaded.len() as f64 / self.total as f64
    }
}

/// Individual beatmapset download result
#[derive(Debug, Clone)]
pub enum DownloadResult {
    /// Download succeeded
    Success {
        /// Downloaded filename
        filename: String,
        /// File size in bytes
        size_bytes: u64,
        /// MD5 hash (if computed)
        md5_hash: Option<String>,
        /// Mirror that served the file
        mirror_used: MirrorKind,
    },
    /// Download was skipped
    Skipped {
        /// Reason for skipping
        reason: SkipReason,
    },
}

impl DownloadResult {
    /// Check if the download was successful
    pub fn is_success(&self) -> bool {
        matches!(self, DownloadResult::Success { .. })
    }

    /// Check if the download was skipped
    pub fn is_skipped(&self) -> bool {
        matches!(self, DownloadResult::Skipped { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_download_summary() {
        let mut summary = DownloadSummary::new(10);
        summary.downloaded = vec![1, 2, 3];
        summary.skipped = vec![(4, SkipReason::AlreadyExists)];
        summary.failed = vec![(5, "Error".to_string())];

        assert_eq!(summary.downloaded_count(), 3);
        assert_eq!(summary.skipped_count(), 1);
        assert_eq!(summary.failed_count(), 1);
        assert!(!summary.all_succeeded());
        assert_eq!(summary.success_rate(), 0.3);
    }

    #[test]
    fn test_download_result() {
        let success = DownloadResult::Success {
            filename: "test.osz".to_string(),
            size_bytes: 1024,
            md5_hash: None,
            mirror_used: crate::MirrorKind::Custom,
        };
        assert!(success.is_success());
        assert!(!success.is_skipped());

        let skipped = DownloadResult::Skipped {
            reason: SkipReason::AlreadyExists,
        };
        assert!(!skipped.is_success());
        assert!(skipped.is_skipped());
    }
}
