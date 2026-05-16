//! Main downloader API

use crate::{
    config::DownloadConfig,
    event::{DownloadResult, DownloadSummary, SkipReason},
    http,
    mirrors::{Mirror, MirrorKind, MirrorPool},
    Error, Result,
};
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::mpsc;

/// Builder for creating a `Downloader`
pub struct DownloaderBuilder {
    mirrors: Vec<Mirror>,
    concurrent_downloads: Option<usize>,
    max_retries: Option<u32>,
    verify_archives: Option<bool>,
    progress_timeout: Option<Duration>,
    user_agent: Option<String>,
    no_video: bool,
    #[cfg(any(test, feature = "test-helpers"))]
    http_client_override: Option<reqwest::Client>,
}

impl DownloaderBuilder {
    /// Create a new downloader builder
    pub fn new() -> Self {
        Self {
            mirrors: Vec::new(),
            concurrent_downloads: None,
            max_retries: None,
            verify_archives: None,
            progress_timeout: None,
            user_agent: None,
            no_video: false,
            #[cfg(any(test, feature = "test-helpers"))]
            http_client_override: None,
        }
    }

    /// Add a mirror to the downloader
    pub fn mirror(mut self, mirror: Mirror) -> Self {
        self.mirrors.push(mirror);
        self
    }

    /// Add multiple mirrors to the downloader
    pub fn mirrors(mut self, mirrors: impl IntoIterator<Item = Mirror>) -> Self {
        self.mirrors.extend(mirrors);
        self
    }

    /// Add default mirrors (Nerinyan, Catboy Central, osu.direct)
    pub fn default_mirrors(mut self) -> Self {
        self.mirrors.push(Mirror::nerinyan());
        self.mirrors
            .push(Mirror::catboy(crate::CatboyRegion::Central));
        self.mirrors.push(Mirror::osu_direct());
        self
    }

    /// Set the number of concurrent downloads (default: 4)
    pub fn concurrent_downloads(mut self, count: usize) -> Self {
        self.concurrent_downloads = Some(count);
        self
    }

    /// Set the maximum number of retries per beatmapset (default: 3)
    pub fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = Some(retries);
        self
    }

    /// Set whether to verify ZIP archives (default: true)
    pub fn verify_archives(mut self, verify: bool) -> Self {
        self.verify_archives = Some(verify);
        self
    }

    /// Set the progress timeout duration (default: 30 seconds)
    pub fn progress_timeout(mut self, timeout: Duration) -> Self {
        self.progress_timeout = Some(timeout);
        self
    }

    /// Set a custom user agent string
    pub fn user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }

    /// Skip video files in beatmapsets (default: false)
    pub fn no_video(mut self, no_video: bool) -> Self {
        self.no_video = no_video;
        self
    }

    /// Override the HTTP client.
    #[cfg(any(test, feature = "test-helpers"))]
    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.http_client_override = Some(client);
        self
    }

    /// Build the downloader
    pub fn build(self) -> Result<Downloader> {
        if self.mirrors.is_empty() {
            return Err(Error::config(
                "At least one mirror must be configured. Use .default_mirrors() or .mirror()",
            ));
        }

        let concurrent_downloads = self.concurrent_downloads.unwrap_or(4);
        if concurrent_downloads == 0 {
            return Err(Error::config(
                "concurrent downloads must be greater than zero",
            ));
        }

        let config = DownloadConfig {
            concurrent_downloads,
            verify_archives: self.verify_archives.unwrap_or(true),
            progress_timeout: self.progress_timeout.unwrap_or(Duration::from_secs(30)),
            user_agent: self
                .user_agent
                .unwrap_or_else(|| format!("osu-downloader/{}", env!("CARGO_PKG_VERSION"))),
        };
        let mirrors = self
            .mirrors
            .into_iter()
            .map(|mirror| mirror.video(self.no_video))
            .collect();

        #[cfg(any(test, feature = "test-helpers"))]
        let http_client = if let Some(client) = self.http_client_override {
            client
        } else {
            http::create_download_client(Some(config.user_agent.clone()))?
        };
        #[cfg(not(any(test, feature = "test-helpers")))]
        let http_client = http::create_download_client(Some(config.user_agent.clone()))?;
        let mirror_pool = MirrorPool::new(mirrors);

        Ok(Downloader {
            config: Arc::new(config),
            http_client,
            mirror_pool: Arc::new(mirror_pool),
        })
    }
}

impl Default for DownloaderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Callbacks emitted while downloading a single beatmapset.
#[derive(Clone, Default)]
pub struct BeatmapsetDownloadCallbacks {
    /// Receives bytes downloaded and total bytes, or `0` when unknown.
    pub progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    /// Receives structured status updates for mirror attempts.
    pub status: Option<Arc<dyn Fn(BeatmapsetStatusEvent) + Send + Sync>>,
}

/// File-exists policy for a single beatmapset download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileExistsPolicy {
    /// Skip if any matching beatmapset archive already exists.
    Skip,
    /// Overwrite the exact target filename if it already exists.
    OverwriteTarget,
}

/// Options for a single beatmapset download.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeatmapsetDownloadOptions {
    /// How to handle an existing target archive.
    pub file_exists_policy: FileExistsPolicy,
}

impl Default for BeatmapsetDownloadOptions {
    fn default() -> Self {
        Self {
            file_exists_policy: FileExistsPolicy::Skip,
        }
    }
}

/// Status update for a single beatmapset download attempt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatmapsetStatusEvent {
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
    /// The downloaded archive is being verified.
    Verifying {
        /// Mirror that served the archive.
        mirror: MirrorKind,
    },
    /// A mirror returned a rate-limit response.
    RateLimited {
        /// Rate-limited mirror.
        mirror: MirrorKind,
        /// Cooldown before the mirror is retried.
        cooldown: Duration,
    },
    /// A transient failure will be retried on the same mirror.
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
    /// A mirror cannot serve this beatmapset for this attempt.
    MirrorFailed {
        /// Failed mirror.
        mirror: MirrorKind,
        /// Failure reason.
        reason: String,
    },
}

/// Outcome for a single beatmapset download.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BeatmapsetDownloadOutcome {
    /// Download succeeded.
    Success {
        /// Downloaded filename.
        filename: String,
        /// MD5 hash of the archive.
        hash: String,
        /// Mirror that served the archive.
        mirror: MirrorKind,
        /// Archive size in bytes.
        size_bytes: u64,
        /// Archive verification time in microseconds.
        verify_duration_us: u64,
    },
    /// Download was skipped.
    Skipped {
        /// Reason for skipping.
        reason: SkipReason,
    },
    /// Download failed for a non-transient reason.
    Failed {
        /// Mirror associated with the failure, if known.
        mirror: Option<MirrorKind>,
        /// Failure reason.
        reason: String,
    },
    /// Every mirror failed with transient network errors only.
    NetworkError {
        /// Last transient failure reason.
        reason: String,
    },
    /// Download was cancelled.
    Aborted,
}

impl From<BeatmapsetDownloadOutcome> for Result<DownloadResult> {
    fn from(outcome: BeatmapsetDownloadOutcome) -> Self {
        match outcome {
            BeatmapsetDownloadOutcome::Success {
                filename,
                hash,
                mirror,
                size_bytes,
                verify_duration_us: _,
            } => Ok(DownloadResult::Success {
                filename,
                size_bytes,
                md5_hash: Some(hash),
                mirror_used: mirror,
            }),
            BeatmapsetDownloadOutcome::Skipped { reason } => Ok(DownloadResult::Skipped { reason }),
            BeatmapsetDownloadOutcome::Failed { reason, .. }
            | BeatmapsetDownloadOutcome::NetworkError { reason } => {
                Err(crate::DownloadError::worker_error(reason).into())
            }
            BeatmapsetDownloadOutcome::Aborted => Err(crate::DownloadError::Cancelled.into()),
        }
    }
}

/// Main downloader client for downloading osu! beatmapsets
pub struct Downloader {
    config: Arc<DownloadConfig>,
    http_client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
}

impl Downloader {
    /// Create a new downloader builder
    pub fn builder() -> DownloaderBuilder {
        DownloaderBuilder::new()
    }

    /// Download a single beatmapset
    ///
    /// # Arguments
    ///
    /// * `beatmapset_id` - The beatmapset ID to download
    /// * `output_dir` - Directory to save the beatmapset
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use osu_downloader::{Downloader, Mirror};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let downloader = Downloader::builder()
    ///     .mirror(Mirror::nerinyan())
    ///     .build()?;
    ///
    /// let result = downloader.download_one(123456, "./downloads").await?;
    /// println!("Downloaded: {:?}", result);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn download_one(
        &self,
        beatmapset_id: u32,
        output_dir: impl AsRef<Path>,
    ) -> Result<DownloadResult> {
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        self.download_beatmapset(
            beatmapset_id,
            self.mirror_pool.mirrors().to_vec().as_slice(),
            output_dir,
            BeatmapsetDownloadCallbacks::default(),
            cancel_rx,
        )
        .await
        .into()
    }

    /// Download a single beatmapset through the selected mirrors.
    pub async fn download_beatmapset(
        &self,
        beatmapset_id: u32,
        mirrors: &[Mirror],
        output_dir: impl AsRef<Path>,
        callbacks: BeatmapsetDownloadCallbacks,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) -> BeatmapsetDownloadOutcome {
        self.download_beatmapset_with_options(
            beatmapset_id,
            mirrors,
            output_dir,
            callbacks,
            BeatmapsetDownloadOptions::default(),
            cancel_rx,
        )
        .await
    }

    /// Download a single beatmapset through the selected mirrors with explicit options.
    pub async fn download_beatmapset_with_options(
        &self,
        beatmapset_id: u32,
        mirrors: &[Mirror],
        output_dir: impl AsRef<Path>,
        callbacks: BeatmapsetDownloadCallbacks,
        options: BeatmapsetDownloadOptions,
        cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) -> BeatmapsetDownloadOutcome {
        let mirror_pool = MirrorPool::new(mirrors.to_vec());
        let (outcome, _retries) =
            crate::download::download_beatmapset(crate::download::DownloadParams {
                beatmapset_id,
                output_dir: output_dir.as_ref(),
                client: &self.http_client,
                mirror_pool: &mirror_pool,
                verify_archive: self.config.verify_archives,
                progress_timeout: self.config.progress_timeout,
                callbacks,
                options,
                cancel_rx,
            })
            .await;
        outcome
    }

    /// Download multiple beatmapsets
    ///
    /// Returns a `DownloadSession` handle for tracking progress and receiving events.
    ///
    /// # Arguments
    ///
    /// * `beatmapset_ids` - Iterable of beatmapset IDs to download
    /// * `output_dir` - Directory to save beatmapsets
    ///
    /// # Example
    ///
    /// ```no_run
    /// # use osu_downloader::{Downloader, Mirror, DownloadEvent};
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// let downloader = Downloader::builder()
    ///     .mirror(Mirror::nerinyan())
    ///     .concurrent_downloads(8)
    ///     .build()?;
    ///
    /// let ids = vec![123456, 789012, 345678];
    /// let mut session = downloader.download_many(ids, "./downloads").await;
    ///
    /// while let Some(event) = session.next_event().await {
    ///     match event {
    ///         DownloadEvent::Progress { beatmapset_id, downloaded_bytes, .. } => {
    ///             println!("#{}: {} bytes", beatmapset_id, downloaded_bytes);
    ///         }
    ///         _ => {}
    ///     }
    /// }
    ///
    /// let summary = session.wait().await?;
    /// println!("Downloaded {}/{}", summary.downloaded.len(), summary.total);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn download_many(
        &self,
        beatmapset_ids: impl IntoIterator<Item = u32>,
        output_dir: impl AsRef<Path>,
    ) -> DownloadSession {
        let ids: Vec<u32> = beatmapset_ids.into_iter().collect();
        let output_dir = output_dir.as_ref().to_path_buf();

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let client = self.http_client.clone();
        let mirror_pool = self.mirror_pool.clone();
        let config = self.config.clone();

        let task = tokio::spawn(async move {
            let batch_config = crate::batch::BatchConfig {
                concurrent_downloads: config.concurrent_downloads,
                verify_archives: config.verify_archives,
                progress_timeout: config.progress_timeout,
            };

            let summary = crate::batch::download_batch(
                ids,
                &output_dir,
                client,
                mirror_pool,
                batch_config,
                event_tx,
                cancel_rx,
            )
            .await;

            Ok(summary)
        });

        DownloadSession {
            events: event_rx,
            cancel: cancel_tx,
            task,
        }
    }
}

/// Handle to a running download session
pub struct DownloadSession {
    events: mpsc::UnboundedReceiver<crate::DownloadEvent>,
    cancel: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<Result<DownloadSummary>>,
}

impl DownloadSession {
    /// Get the next download event
    ///
    /// Returns `None` when the session has completed.
    pub async fn next_event(&mut self) -> Option<crate::DownloadEvent> {
        self.events.recv().await
    }

    /// Cancel the download session
    pub fn cancel(&self) {
        let _ = self.cancel.send(true);
    }

    /// Wait for the session to complete and get the summary
    ///
    /// This consumes the session handle.
    pub async fn wait(mut self) -> Result<DownloadSummary> {
        // Drain remaining events
        while self.events.recv().await.is_some() {}

        self.task
            .await
            .map_err(|e| Error::config(format!("Download task panicked: {}", e)))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let builder = Downloader::builder()
            .mirror(Mirror::nerinyan())
            .concurrent_downloads(8)
            .max_retries(5)
            .verify_archives(true)
            .no_video(true);

        let downloader = builder.build();
        assert!(downloader.is_ok());
    }

    #[test]
    fn test_builder_no_mirrors() {
        let builder = Downloader::builder();
        let result = builder.build();
        assert!(result.is_err());
    }

    #[test]
    fn test_builder_rejects_zero_concurrency() {
        let result = Downloader::builder()
            .mirror(Mirror::nerinyan())
            .concurrent_downloads(0)
            .build();

        assert!(matches!(result, Err(Error::Config(_))));
    }

    #[test]
    fn test_builder_default_mirrors() {
        let builder = Downloader::builder().default_mirrors();
        let result = builder.build();
        assert!(result.is_ok());
    }

    #[test]
    fn test_builder_applies_no_video_to_builtin_mirrors() {
        let downloader = Downloader::builder()
            .mirror(Mirror::nerinyan())
            .mirror(Mirror::custom("https://example.com/d/{id}").unwrap())
            .no_video(true)
            .build()
            .unwrap();

        assert_eq!(
            downloader.mirror_pool.mirrors()[0].url_for(123),
            "https://api.nerinyan.moe/d/123?nv=1"
        );
        assert_eq!(
            downloader.mirror_pool.mirrors()[1].url_for(123),
            "https://example.com/d/123"
        );
    }

    #[test]
    fn test_builder_preserves_builtin_headers_with_no_video() {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(
            reqwest::header::AUTHORIZATION,
            reqwest::header::HeaderValue::from_static("bearer token"),
        );

        let downloader = Downloader::builder()
            .mirror(Mirror::nerinyan().set_headers(headers.clone()))
            .no_video(true)
            .build()
            .unwrap();

        assert_eq!(
            downloader.mirror_pool.mirrors()[0].headers(),
            Some(&headers)
        );
        assert_eq!(
            downloader.mirror_pool.mirrors()[0].url_for(123),
            "https://api.nerinyan.moe/d/123?nv=1"
        );
    }
}
