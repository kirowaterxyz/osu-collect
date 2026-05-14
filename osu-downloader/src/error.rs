use std::io;
use thiserror::Error;

/// Main result type for the library
pub type Result<T> = std::result::Result<T, Error>;

/// Top-level error type for osu-downloader
#[derive(Debug, Error)]
pub enum Error {
    /// HTTP/network error
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),

    /// I/O error (file system operations)
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    /// Invalid mirror configuration
    #[error("Invalid mirror configuration: {0}")]
    InvalidMirror(String),

    /// Invalid downloader configuration
    #[error("Invalid configuration: {0}")]
    Config(String),

    /// Download operation failed
    #[error("Download failed: {0}")]
    Download(#[from] DownloadError),

    /// Collection API error (feature: collection)
    #[cfg(feature = "collection")]
    #[error("Collection API error: {0}")]
    Collection(String),

    /// JSON parsing error
    #[error("JSON parsing error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Download-specific error types
#[derive(Debug, Clone, Error)]
pub enum DownloadError {
    /// All configured mirrors failed for a beatmapset
    #[error("All mirrors failed for beatmapset {beatmapset_id}")]
    AllMirrorsFailed {
        /// The beatmapset ID that failed
        beatmapset_id: u32,
    },

    /// Archive validation failed (invalid ZIP format or hash mismatch)
    #[error("Archive validation failed: {reason}")]
    ValidationFailed {
        /// Reason for validation failure
        reason: String,
    },

    /// Download progress timed out
    #[error("Progress timeout exceeded")]
    ProgressTimeout,

    /// Beatmapset not found on any configured mirror
    #[error("Beatmapset not found on any mirror")]
    NotFound,

    /// All mirrors are currently rate limited
    #[error("Rate limited by all mirrors")]
    RateLimited,

    /// Download was cancelled by user
    #[error("Cancelled by user")]
    Cancelled,

    /// Worker thread error
    #[error("Worker error: {0}")]
    WorkerError(String),

    /// Non-success HTTP status code received during download
    #[error("HTTP {0}")]
    HttpStatus(u16),

    /// HTTP/network error during download
    #[error("HTTP error: {0}")]
    Http(String),

    /// Response body stream failed during download
    #[error("Stream error: {0}")]
    Stream(String),

    /// I/O error during download
    #[error("I/O error: {0}")]
    Io(String),
}

impl Error {
    pub(crate) fn invalid_mirror(msg: impl Into<String>) -> Self {
        Error::InvalidMirror(msg.into())
    }

    pub(crate) fn config(msg: impl Into<String>) -> Self {
        Error::Config(msg.into())
    }

    #[cfg(feature = "collection")]
    pub(crate) fn collection(msg: impl Into<String>) -> Self {
        Error::Collection(msg.into())
    }
}

impl DownloadError {
    pub(crate) fn validation_failed(reason: impl Into<String>) -> Self {
        DownloadError::ValidationFailed {
            reason: reason.into(),
        }
    }

    pub(crate) fn worker_error(msg: impl Into<String>) -> Self {
        DownloadError::WorkerError(msg.into())
    }

    pub(crate) fn http(msg: impl Into<String>) -> Self {
        DownloadError::Http(msg.into())
    }

    pub(crate) fn stream(msg: impl Into<String>) -> Self {
        DownloadError::Stream(msg.into())
    }

    pub(crate) fn io(msg: impl Into<String>) -> Self {
        DownloadError::Io(msg.into())
    }
}

impl From<reqwest::Error> for DownloadError {
    fn from(err: reqwest::Error) -> Self {
        DownloadError::Http(err.to_string())
    }
}

impl From<io::Error> for DownloadError {
    fn from(err: io::Error) -> Self {
        DownloadError::Io(err.to_string())
    }
}
