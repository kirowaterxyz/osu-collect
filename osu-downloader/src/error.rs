//! Unified error type and result alias.

use std::time::Duration;
use thiserror::Error;

/// Library-wide result alias.
pub type Result<T> = std::result::Result<T, Error>;

/// All errors surfaced by this crate.
///
/// `Clone` is implemented so events that carry an error can be cheaply forwarded
/// across channels. Inner I/O / HTTP errors are flattened to strings for that reason.
#[derive(Debug, Clone, Error)]
pub enum Error {
    /// Builder rejected the configuration.
    #[error("configuration error: {0}")]
    Config(String),

    /// A mirror template or URL was rejected.
    #[error("invalid mirror: {0}")]
    Mirror(String),

    /// Archive validation failed (bad ZIP, wrong content, etc.).
    #[error("archive validation failed: {0}")]
    Validation(String),

    /// Transport-level failure: connect/read/decode/stream/etc.
    #[error("network error: {0}")]
    Network(String),

    /// Stall watchdog fired or a connect/read timed out.
    #[error("operation timed out")]
    Timeout,

    /// Resource not found (HTTP 404).
    #[error("not found")]
    NotFound,

    /// Server returned HTTP 429. `retry_after` carries the `Retry-After` header when present.
    #[error("rate limited")]
    RateLimited {
        /// Cooldown the server asked the client to wait, if any.
        retry_after: Option<Duration>,
    },

    /// Server returned an unsuccessful status code not covered by other variants.
    #[error("HTTP {0}")]
    HttpStatus(u16),

    /// The caller-supplied URL could not be parsed.
    #[error("invalid URL: {0}")]
    InvalidUrl(String),

    /// JSON (or other body) failed to decode.
    #[error("parse error: {0}")]
    Parse(String),

    /// Operation was cancelled by the caller.
    #[error("cancelled by caller")]
    Cancelled,

    /// Local I/O failure.
    #[error("I/O error: {0}")]
    Io(String),
}

impl Error {
    pub(crate) fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    pub(crate) fn mirror(msg: impl Into<String>) -> Self {
        Self::Mirror(msg.into())
    }

    pub(crate) fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub(crate) fn network(msg: impl Into<String>) -> Self {
        Self::Network(msg.into())
    }

    pub(crate) fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }

    pub(crate) fn invalid_url(msg: impl Into<String>) -> Self {
        Self::InvalidUrl(msg.into())
    }

    /// True for variants that the library considers transient.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Error::Network(_) | Error::Timeout | Error::RateLimited { .. }
        )
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Io(err.to_string())
    }
}

impl From<reqwest::Error> for Error {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            Error::Timeout
        } else {
            Error::Network(err.to_string())
        }
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::Parse(err.to_string())
    }
}
