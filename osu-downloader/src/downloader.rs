//! Public download API.
//!
//! [`Downloader`] is the only entrypoint. Build one with [`DownloaderBuilder`], then call
//! [`Downloader::download_many`] to start a session and consume its event stream.

use crate::{
    batch::{self, BatchConfig},
    config::DownloadConfig,
    event::DownloadSummary,
    http,
    mirrors::{Mirror, MirrorKind, MirrorPool},
    DownloadEvent, Error, Result,
};
use futures_util::Stream;
use std::{path::Path, sync::Arc, time::Duration};
use tokio::sync::{mpsc, watch};
use tokio_stream::wrappers::UnboundedReceiverStream;

/// Builder for a [`Downloader`].
pub struct DownloaderBuilder {
    mirrors: Vec<Mirror>,
    concurrent_downloads: Option<usize>,
    verify_archives: Option<bool>,
    progress_timeout: Option<Duration>,
    user_agent: Option<String>,
    no_video: bool,
    network_retry_attempts: usize,
    #[cfg(any(test, feature = "test-helpers"))]
    http_client_override: Option<reqwest::Client>,
}

impl DownloaderBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self {
            mirrors: Vec::new(),
            concurrent_downloads: None,
            verify_archives: None,
            progress_timeout: None,
            user_agent: None,
            no_video: false,
            network_retry_attempts: 0,
            #[cfg(any(test, feature = "test-helpers"))]
            http_client_override: None,
        }
    }

    /// Add a mirror.
    #[must_use]
    pub fn mirror(mut self, mirror: Mirror) -> Self {
        self.mirrors.push(mirror);
        self
    }

    /// Add several mirrors.
    #[must_use]
    pub fn mirrors(mut self, mirrors: impl IntoIterator<Item = Mirror>) -> Self {
        self.mirrors.extend(mirrors);
        self
    }

    /// Add the built-in mirrors.
    #[must_use]
    pub fn default_mirrors(mut self) -> Self {
        self.mirrors.push(Mirror::nerinyan());
        self.mirrors.push(Mirror::osu_direct());
        self.mirrors.push(Mirror::sayobot());
        self.mirrors.push(Mirror::nekoha());
        self
    }

    /// Set max concurrent downloads (default 4).
    #[must_use]
    pub fn concurrent_downloads(mut self, count: usize) -> Self {
        self.concurrent_downloads = Some(count);
        self
    }

    /// Toggle ZIP archive verification (default true).
    #[must_use]
    pub fn verify_archives(mut self, verify: bool) -> Self {
        self.verify_archives = Some(verify);
        self
    }

    /// Stall-watchdog for the body stream (default 30s).
    #[must_use]
    pub fn progress_timeout(mut self, timeout: Duration) -> Self {
        self.progress_timeout = Some(timeout);
        self
    }

    /// Custom User-Agent.
    #[must_use]
    pub fn user_agent(mut self, agent: impl Into<String>) -> Self {
        self.user_agent = Some(agent.into());
        self
    }

    /// Strip video from beatmapsets where the mirror supports it.
    #[must_use]
    pub fn no_video(mut self, no_video: bool) -> Self {
        self.no_video = no_video;
        self
    }

    /// Maximum number of additional passes through the mirror pool after every
    /// mirror has exhausted its transient retries. Each retry waits 5 seconds
    /// (cancellable) before the next pass. Default `0` (no retry).
    #[must_use]
    pub fn network_retry_attempts(mut self, attempts: usize) -> Self {
        self.network_retry_attempts = attempts;
        self
    }

    /// Override the HTTP client (test helper).
    #[cfg(any(test, feature = "test-helpers"))]
    #[must_use]
    pub fn client(mut self, client: reqwest::Client) -> Self {
        self.http_client_override = Some(client);
        self
    }

    /// Build the [`Downloader`].
    pub fn build(self) -> Result<Downloader> {
        if self.mirrors.is_empty() {
            return Err(Error::config(
                "at least one mirror must be configured (use .default_mirrors() or .mirror())",
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
            network_retry_attempts: self.network_retry_attempts,
        };
        let mirrors: Vec<Mirror> = self
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

        Ok(Downloader {
            config: Arc::new(config),
            http_client,
            mirror_pool: Arc::new(MirrorPool::new(mirrors)),
        })
    }
}

impl Default for DownloaderBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// How to handle a beatmapset whose target archive already exists on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileExistsPolicy {
    /// Skip if any matching archive is already present.
    Skip,
    /// Overwrite the exact target filename.
    OverwriteTarget,
}

/// A single item the [`Downloader`] should attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DownloadItem {
    /// Beatmapset ID to fetch.
    pub beatmapset_id: u32,
    /// Behaviour when the target file already exists.
    pub policy: FileExistsPolicy,
}

impl DownloadItem {
    /// Skip if already present (the common case).
    pub fn skip_if_present(beatmapset_id: u32) -> Self {
        Self {
            beatmapset_id,
            policy: FileExistsPolicy::Skip,
        }
    }

    /// Overwrite any existing target file.
    pub fn overwrite(beatmapset_id: u32) -> Self {
        Self {
            beatmapset_id,
            policy: FileExistsPolicy::OverwriteTarget,
        }
    }
}

impl From<u32> for DownloadItem {
    fn from(beatmapset_id: u32) -> Self {
        Self::skip_if_present(beatmapset_id)
    }
}

/// Status update emitted while a single beatmapset is being attempted.
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
    /// The archive is being verified.
    Verifying {
        /// Mirror that served the archive.
        mirror: MirrorKind,
    },
    /// A mirror returned a rate-limit response.
    RateLimited {
        /// Rate-limited mirror.
        mirror: MirrorKind,
        /// Cooldown before that mirror will be retried.
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
    /// A mirror cannot serve this beatmapset for this attempt.
    MirrorFailed {
        /// Failed mirror.
        mirror: MirrorKind,
        /// Failure reason.
        reason: String,
    },
}

/// The downloader. Cheap to clone via `Arc` if desired.
pub struct Downloader {
    config: Arc<DownloadConfig>,
    http_client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
}

impl Downloader {
    /// New builder.
    pub fn builder() -> DownloaderBuilder {
        DownloaderBuilder::new()
    }

    /// Start a batch download. Returns a [`DownloadSession`] for events + cancel.
    pub fn download_many(
        &self,
        items: impl IntoIterator<Item = impl Into<DownloadItem>>,
        output_dir: impl AsRef<Path>,
    ) -> DownloadSession {
        let items: Vec<DownloadItem> = items.into_iter().map(Into::into).collect();
        let output_dir = output_dir.as_ref().to_path_buf();
        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = watch::channel(false);

        let client = self.http_client.clone();
        let mirror_pool = self.mirror_pool.clone();
        let config = self.config.clone();

        let task = tokio::spawn(async move {
            let batch_config = BatchConfig {
                concurrent_downloads: config.concurrent_downloads,
                verify_archives: config.verify_archives,
                progress_timeout: config.progress_timeout,
                network_retry_attempts: config.network_retry_attempts,
            };
            batch::download_batch(
                items,
                &output_dir,
                client,
                mirror_pool,
                batch_config,
                event_tx,
                cancel_rx,
            )
            .await
        });

        DownloadSession {
            events: Some(event_rx),
            cancel: cancel_tx,
            task,
        }
    }
}

/// Handle to a running batch session.
pub struct DownloadSession {
    events: Option<mpsc::UnboundedReceiver<DownloadEvent>>,
    cancel: watch::Sender<bool>,
    task: tokio::task::JoinHandle<DownloadSummary>,
}

impl DownloadSession {
    /// Consume the event stream. Can only be called once per session.
    pub fn events(&mut self) -> impl Stream<Item = DownloadEvent> + Unpin + Send + 'static {
        let rx = self
            .events
            .take()
            .expect("events() can only be called once");
        UnboundedReceiverStream::new(rx)
    }

    /// Signal cancellation. Running downloads abort at the next checkpoint.
    pub fn cancel(&self) {
        let _ = self.cancel.send(true);
    }

    /// Wait for the task to finish and return the [`DownloadSummary`]. Drops any remaining events.
    pub async fn wait(mut self) -> Result<DownloadSummary> {
        if let Some(mut rx) = self.events.take() {
            while rx.recv().await.is_some() {}
        }
        self.task
            .await
            .map_err(|err| Error::config(format!("download task panicked: {err}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_requires_at_least_one_mirror() {
        assert!(Downloader::builder().build().is_err());
    }

    #[test]
    fn builder_rejects_zero_concurrency() {
        let result = Downloader::builder()
            .mirror(Mirror::nerinyan())
            .concurrent_downloads(0)
            .build();
        assert!(matches!(result, Err(Error::Config(_))));
    }

    #[test]
    fn default_mirrors_include_every_builtin_mirror() {
        let downloader = Downloader::builder().default_mirrors().build().unwrap();

        let mirror_kinds: Vec<_> = downloader
            .mirror_pool
            .mirrors()
            .iter()
            .map(Mirror::kind)
            .collect();
        assert_eq!(
            mirror_kinds,
            vec![
                MirrorKind::Nerinyan,
                MirrorKind::OsuDirect,
                MirrorKind::Sayobot,
                MirrorKind::Nekoha,
            ]
        );
    }

    #[test]
    fn builder_applies_no_video_to_builtin_mirrors() {
        let downloader = Downloader::builder()
            .mirror(Mirror::nerinyan())
            .mirror(Mirror::custom("https://example.com/d/{id}").unwrap())
            .no_video(true)
            .build()
            .unwrap();

        let mirrors = downloader.mirror_pool.mirrors();
        assert_eq!(
            mirrors[0].url_for(123),
            "https://api.nerinyan.moe/d/123?nv=1"
        );
        assert_eq!(mirrors[1].url_for(123), "https://example.com/d/123");
    }
}
