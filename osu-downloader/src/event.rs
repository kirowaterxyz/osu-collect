//! Event types emitted during a download session.

use crate::{DownloadError, MirrorKind};
use std::time::Duration;

/// Events emitted while a [`DownloadSession`](crate::DownloadSession) is running.
#[derive(Debug, Clone)]
pub enum Event {
    /// Session has started.
    SessionStarted {
        /// Total number of beatmapsets in the session.
        total_beatmapsets: usize,
    },

    /// A beatmapset download has started.
    BeatmapsetStarted {
        /// Beatmapset ID.
        beatmapset_id: u32,
    },

    /// Per-attempt status update (which mirror is being contacted, rate limits, etc.).
    BeatmapsetStatus {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Underlying status event.
        status: StatusEvent,
    },

    /// Download progress update.
    Progress {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Bytes downloaded so far.
        downloaded_bytes: u64,
        /// Total bytes if the server reported a Content-Length.
        total_bytes: Option<u64>,
        /// Bytes-per-second since the last progress emission.
        speed_bps: u64,
    },

    /// A beatmapset was downloaded successfully.
    BeatmapsetCompleted {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// On-disk filename.
        filename: String,
        /// File size in bytes.
        size_bytes: u64,
        /// MD5 hash if computed.
        md5_hash: Option<String>,
        /// Mirror that served the archive.
        mirror_used: MirrorKind,
        /// Archive verification time in microseconds.
        verify_duration_us: u64,
    },

    /// A beatmapset failed for a non-transient reason.
    BeatmapsetFailed {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Underlying error.
        error: DownloadError,
        /// Mirror associated with the failure if known.
        mirror: Option<MirrorKind>,
    },

    /// Every mirror failed with transient network errors only.
    BeatmapsetNetworkError {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Last transient failure reason.
        reason: String,
    },

    /// A beatmapset was skipped.
    BeatmapsetSkipped {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Reason for skipping.
        reason: SkipReason,
    },

    /// Session has finished.
    SessionCompleted {
        /// Aggregate summary.
        summary: Summary,
    },
}

/// Per-attempt status update emitted while a single beatmapset is being attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusEvent {
    /// A mirror is being contacted.
    Contacting {
        /// Mirror being contacted.
        mirror: MirrorKind,
    },
    /// A mirror started streaming the archive.
    Downloading {
        /// Mirror serving the archive.
        mirror: MirrorKind,
    },
    /// The archive is being verified.
    Verifying {
        /// Mirror that served the archive.
        mirror: MirrorKind,
    },
    /// Every untried mirror is currently rate-limited; the attempt is paused
    /// until the shortest cooldown elapses.
    RateLimited {
        /// Cooldown before any rate-limited mirror becomes eligible again.
        cooldown: Duration,
    },
    /// A transient error will be retried on the same mirror.
    RetryingTransient {
        /// Mirror being retried.
        mirror: MirrorKind,
        /// Attempt about to run.
        attempt: u32,
        /// Maximum attempts for this mirror.
        max_attempts: u32,
        /// Failure reason.
        reason: String,
    },
}

/// Reason a beatmapset was skipped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SkipReason {
    /// Already exists at the destination.
    AlreadyExists,
    /// Not available on any configured mirror.
    UnavailableOnMirrors,
    /// Caller marked the item as not-to-download.
    InvalidBeatmapsetId,
}

/// Summary of a completed download session.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    /// Total number of beatmapsets requested.
    pub total: usize,
    /// IDs of successful downloads.
    pub downloaded: Vec<u32>,
    /// IDs of skipped beatmapsets with reasons.
    pub skipped: Vec<(u32, SkipReason)>,
    /// IDs of failed beatmapsets with error messages.
    pub failed: Vec<(u32, String)>,
    /// IDs of beatmapsets that gave up after all mirrors hit transient errors.
    pub network_errors: Vec<u32>,
    /// Total bytes downloaded.
    pub total_bytes: u64,
    /// Session duration.
    pub duration: Duration,
}

impl Summary {
    pub(crate) fn new(total: usize) -> Self {
        Self {
            total,
            ..Self::default()
        }
    }

    /// True if every beatmapset succeeded or was skipped because it already existed.
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty() && self.network_errors.is_empty()
    }
}
