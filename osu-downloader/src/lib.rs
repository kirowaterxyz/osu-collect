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
    ArchiveValidation, ArchiveValidationOptions, ArchiveValidationResult, ensure_valid_archive,
    validate_archive,
};
pub use worker::{DownloadStreamResult, stream_download};
