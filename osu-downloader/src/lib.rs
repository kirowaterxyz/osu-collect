//! # osu-downloader
//!
//! A lightweight, reusable library for downloading osu! beatmaps from multiple mirrors.
//!
//! ## Features
//!
//! - Download single or multiple beatmapsets concurrently
//! - Multiple mirror support with automatic fallback
//! - Rate limit handling with automatic backoff
//! - Progress tracking via channel-based events
//! - MD5 hash computation and archive validation
//! - Optional collection support (feature: `collection`)
//! - Optional size fetching (feature: `size-fetch`)
//!
//! ## Quick Start
//!
//! ```rust,no_run
//! use osu_downloader::{Downloader, Mirror};
//!
//! # async fn example() -> Result<(), osu_downloader::Error> {
//! let downloader = Downloader::builder()
//!     .mirror(Mirror::nerinyan())
//!     .mirror(Mirror::catboy(osu_downloader::CatboyRegion::Us))
//!     .concurrent_downloads(8)
//!     .build()?;
//!
//! let mut session = downloader
//!     .download_many(vec![123456, 789012], "./downloads")
//!     .await;
//!
//! while let Some(event) = session.next_event().await {
//!     // Handle download events
//! }
//!
//! let summary = session.wait().await?;
//! println!("Downloaded {} beatmapsets", summary.downloaded.len());
//! # Ok(())
//! # }
//! ```

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

pub use downloader::{DownloadSession, Downloader, DownloaderBuilder};
pub use error::{DownloadError, Error, Result};
pub use event::{DownloadEvent, DownloadResult, DownloadSummary, SkipReason};
pub use mirrors::{CatboyRegion, Mirror, MirrorKind, MirrorPool};

/// Extracts a filename from a Content-Disposition header.
pub fn filename_from_content_disposition(header_value: &str) -> Option<String> {
    download::extract_filename_from_header(header_value)
}
