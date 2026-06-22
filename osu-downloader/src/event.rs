//! Event types emitted during a download session.

use crate::{Error, MirrorKind, MirrorRef};
use std::time::Duration;

/// Events emitted while a [`Session`](crate::Session) is running.
#[derive(Debug, Clone)]
pub enum Event {
    /// Session has started.
    SessionStarted {
        /// Total number of beatmapsets in the session.
        total: usize,
    },

    /// Per-attempt status update (which mirror is being contacted, rate limits, etc.).
    BeatmapsetStatus {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Underlying status update.
        status: Status,
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
        mirror_used: MirrorRef,
        /// Archive verification time in microseconds.
        verify_duration_us: u64,
    },

    /// A beatmapset failed.
    ///
    /// Transient/network failures that exhausted every mirror also arrive here,
    /// carrying [`Error::Network`] (with [`Error::is_transient`] returning true).
    BeatmapsetFailed {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Underlying error.
        error: Error,
        /// Mirror associated with the failure if known. `None` when every
        /// mirror was tried (e.g. all-transient case).
        mirror: Option<MirrorKind>,
    },

    /// A beatmapset was skipped.
    BeatmapsetSkipped {
        /// Beatmapset ID.
        beatmapset_id: u32,
        /// Reason for skipping.
        reason: Skip,
    },

    /// Session has finished.
    SessionCompleted {
        /// Aggregate summary.
        summary: Summary,
    },
}

/// Per-attempt status update emitted while a single beatmapset is being attempted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// A mirror is being contacted.
    Contacting {
        /// Mirror being contacted.
        mirror: MirrorRef,
    },
    /// A mirror started streaming the archive.
    Downloading {
        /// Mirror serving the archive.
        mirror: MirrorRef,
    },
    /// The archive is being verified.
    Verifying {
        /// Mirror that served the archive.
        mirror: MirrorRef,
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
        mirror: MirrorRef,
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
pub enum Skip {
    /// Already exists at the destination.
    AlreadyExists,
    /// Not available on any configured mirror.
    UnavailableOnMirrors,
    /// The caller asked to skip this map while it was waiting on a mirror
    /// rate-limit cooldown (see [`Session::skip_rate_limited`](crate::Session::skip_rate_limited)).
    RateLimitSkipped,
}

/// Summary of a completed download session.
#[derive(Debug, Clone, Default)]
pub struct Summary {
    /// Total number of beatmapsets requested.
    pub total: usize,
    /// IDs of successful downloads.
    pub downloaded: Vec<u32>,
    /// IDs of skipped beatmapsets with reasons.
    pub skipped: Vec<(u32, Skip)>,
    /// IDs of failed beatmapsets paired with the final error.
    /// Includes both definitive failures (404 on every mirror, validation
    /// errors, etc.) and transient failures that exhausted every mirror —
    /// the latter carry [`Error::Network`] / [`Error::Timeout`] / etc.
    pub failed: Vec<(u32, Error)>,
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

    /// True if no beatmapset failed (only `downloaded` and `skipped` entries).
    pub fn all_succeeded(&self) -> bool {
        self.failed.is_empty()
    }
}
