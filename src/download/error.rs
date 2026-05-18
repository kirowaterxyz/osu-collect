use std::io;
use thiserror::Error;

use crate::utils::AppError;

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Rate limited")]
    RateLimited,

    #[error("Not found: {0}")]
    NotFound(Box<str>),

    #[error("Invalid archive: {0}")]
    InvalidArchive(Box<str>),

    #[error("Validation failed for beatmapset {beatmapset_id}: {reason}")]
    ValidationFailed {
        beatmapset_id: u32,
        reason: Box<str>,
    },

    #[error("Disk full: {0}")]
    DiskFull(Box<str>),

    #[error("Download aborted")]
    Aborted,

    #[error("Timeout: {0}")]
    Timeout(Box<str>),

    #[error("IO error: {0}")]
    Io(#[from] io::Error),

    #[error("No mirrors available")]
    NoMirrors,

    #[error("No beatmapsets selected")]
    NoBeatmapsets,

    #[error("Collection is empty")]
    EmptyCollection,

    #[error("Directory not empty")]
    DirectoryNotEmpty,

    #[error("Concurrent download in progress for: {0}")]
    ConcurrentDownload(String),

    #[error("Worker panicked: {0}")]
    WorkerPanic(Box<str>),

    #[error("Internal error: {0}")]
    Internal(Box<str>),
}

impl DownloadError {
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DownloadError::RateLimited | DownloadError::Timeout(_) | DownloadError::Network(_)
        )
    }

    #[inline]
    pub fn not_found(msg: impl Into<Box<str>>) -> Self {
        Self::NotFound(msg.into())
    }

    #[inline]
    pub fn invalid_archive(msg: impl Into<Box<str>>) -> Self {
        Self::InvalidArchive(msg.into())
    }

    #[inline]
    pub fn disk_full(msg: impl Into<Box<str>>) -> Self {
        Self::DiskFull(msg.into())
    }

    #[inline]
    pub fn timeout(msg: impl Into<Box<str>>) -> Self {
        Self::Timeout(msg.into())
    }

    #[inline]
    pub fn worker_panic(msg: impl Into<Box<str>>) -> Self {
        Self::WorkerPanic(msg.into())
    }

    #[inline]
    pub fn internal(msg: impl Into<Box<str>>) -> Self {
        Self::Internal(msg.into())
    }
}

impl From<AppError> for DownloadError {
    fn from(err: AppError) -> Self {
        match err {
            AppError::Network(e) => Self::internal(e.to_string()),
            AppError::FileSystem(e) => Self::internal(e.to_string()),
            AppError::Parsing(e) => Self::internal(e.to_string()),
            AppError::Config(e) => Self::internal(e.to_string()),
            AppError::Domain(e) => Self::internal(e.to_string()),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/error.rs"]
mod tests;
