//! # osu-downloader
//!
//! Library for downloading osu! beatmapsets from multiple mirrors.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use osu_downloader::{Downloader, DownloadItem, Mirror};
//! use futures_util::StreamExt;
//!
//! # async fn example() -> Result<(), osu_downloader::Error> {
//! let downloader = Downloader::builder()
//!     .mirror(Mirror::nerinyan())
//!     .mirror(Mirror::osu_direct())
//!     .concurrent_downloads(8)
//!     .build()?;
//!
//! let items = [123456u32, 789012].map(DownloadItem::skip_if_present);
//! let mut session = downloader.download_many(items, "./downloads");
//! let mut events = session.events();
//! while let Some(_event) = events.next().await {
//!     // handle event
//! }
//! let summary = session.wait().await?;
//! println!("downloaded {} beatmapsets", summary.downloaded.len());
//! # Ok(())
//! # }
//! ```
//!
//! ## Features
//!
//! - `collection` — osucollector.com client and `collection.db` writer
//! - `size-fetch` — Nekoha-backed beatmapset size and availability probes
//! - `test-helpers` — internal helpers for downstream integration tests

#![deny(missing_docs)]

pub(crate) mod batch;
pub(crate) mod config;
pub(crate) mod download;
mod downloader;
mod error;
mod event;
pub mod http;
mod mirrors;
pub(crate) mod validation;
pub(crate) mod worker;

#[cfg(feature = "collection")]
pub mod collection;

#[cfg(feature = "size-fetch")]
pub mod size;

pub use downloader::{
    BeatmapsetStatusEvent, DownloadItem, DownloadSession, Downloader, DownloaderBuilder,
    FileExistsPolicy,
};
pub use error::{DownloadError, Error, Result};
pub use event::{DownloadEvent, DownloadResult, DownloadSummary, SkipReason};
pub use mirrors::{Mirror, MirrorKind};
pub use validation::{
    ensure_valid_archive, validate_archive, ArchiveValidation, ArchiveValidationOptions,
    ArchiveValidationResult,
};
pub use worker::{stream_download, DownloadStreamResult};

/// Internal items exposed for downstream integration tests.
///
/// Gated on the `test-helpers` feature — not part of the public API.
#[cfg(feature = "test-helpers")]
#[doc(hidden)]
pub mod __test_exports {
    pub use crate::batch::{download_batch, BatchConfig};
    #[cfg(feature = "collection")]
    pub use crate::collection::parse_collection_id_from_url;
    pub use crate::download::{
        download_beatmapset, extract_filename_from_header, finalize_download,
        is_archive_content_type, matches_beatmapset, probe_download_size, sanitize_filename,
        size_from_content_range, sleep_cancelable, BeatmapsetDownloadCallbacks,
        BeatmapsetDownloadOptions, BeatmapsetDownloadOutcome, DownloadParams, FinalizeResult,
    };
    pub use crate::mirrors::pool::MirrorPool;
    pub use crate::validation::{
        find_eocd_position, minimal_zip_bytes_for_test, EOCD_SIGNATURE, LOCAL_HEADER_SIGNATURE,
    };
    pub use crate::worker::{TempFileGuard, MIN_PROGRESS_DELTA, TEMP_FILE_COUNTER};
}
