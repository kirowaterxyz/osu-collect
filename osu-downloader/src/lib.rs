//! # osu-downloader
//!
//! library for downloading osu! beatmapsets from multiple mirrors.
//!
//! ## quick start
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
//! ## errors
//!
//! Every fallible call returns the same [`Error`] enum. `Error::is_transient()` is
//! the shortcut for "should I retry?" — true for network, timeout, and rate-limit
//! variants. Collection and size-fetch operations surface the same type.
//!
//! ## features
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
pub(crate) mod http;
mod mirrors;
pub(crate) mod validation;
pub(crate) mod worker;

#[cfg(feature = "collection")]
pub mod collection;

#[cfg(feature = "size-fetch")]
pub mod size;

pub use download::sanitize_filename;
pub use downloader::{
    DownloadItem, DownloadSession, Downloader, DownloaderBuilder, FileExistsPolicy,
};
pub use error::{Error, Result};
pub use event::{Event, SkipReason, StatusEvent, Summary};
pub use mirrors::{Mirror, MirrorKind};
pub use validation::{
    ArchiveValidation, ArchiveValidationResult, validate_and_remove, validate_archive,
};
