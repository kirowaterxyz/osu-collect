use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::download::{BeatmapTracker, CleanupTracker, DownloadEvent, DownloadId, ShutdownToken};
use crate::mirrors::MirrorPool;
use dashmap::DashSet;
use tokio::sync::mpsc::UnboundedSender;

/// Configuration parameters for creating a DownloadContext
pub struct DownloadContextConfig {
    pub id: DownloadId,
    pub thread_count: usize,
    pub skip_existing: bool,
    pub auto_overwrite: bool,
    pub verify_zip_eocd: bool,
    pub shutdown: ShutdownToken,
    pub client: reqwest::Client,
    pub mirror_pool: MirrorPool,
    pub output_dir: PathBuf,
    pub tracker: BeatmapTracker,
    pub initial_unverified: Arc<DashSet<u32>>,
    pub status: StatusSink,
    pub progress_watchdog: Duration,
}

#[derive(Clone)]
pub struct StatusSink {
    inner: Arc<dyn Fn(DownloadEvent) + Send + Sync>,
}

impl StatusSink {
    pub fn from_sender(tx: UnboundedSender<DownloadEvent>) -> Self {
        Self::from_fn(move |event| {
            let _ = tx.send(event);
        })
    }

    pub fn from_fn<F>(callback: F) -> Self
    where
        F: Fn(DownloadEvent) + Send + Sync + 'static,
    {
        Self {
            inner: Arc::new(callback),
        }
    }

    pub fn noop() -> Self {
        Self::from_fn(|_| {})
    }

    pub fn emit(&self, event: DownloadEvent) {
        (self.inner)(event);
    }

    pub fn log(&self, id: DownloadId, message: impl Into<String>) {
        self.emit(DownloadEvent::Log { id, message: message.into() });
    }

    pub fn stage(&self, id: DownloadId, stage: crate::download::DownloadStage) {
        self.emit(DownloadEvent::StageChanged { id, stage });
    }

    pub fn fail(&self, id: DownloadId, message: impl Into<String>) {
        self.emit(DownloadEvent::Failed { id, message: message.into() });
    }

    pub fn finished(&self, id: DownloadId, summary: &crate::download::DownloadSummary) {
        self.emit(DownloadEvent::Finished { id, summary: summary.clone() });
    }

    pub fn target(&self, id: DownloadId, remaining: usize) {
        self.emit(DownloadEvent::DownloadTarget { id, remaining });
    }

    pub fn progress(&self, id: DownloadId, summary: &crate::download::DownloadSummary) {
        self.emit(DownloadEvent::OverallProgress {
            id,
            downloaded: summary.downloaded,
            skipped: summary.skipped,
            failed: summary.failed,
            unverified: summary.unverified,
        });
    }

    pub fn verified_sizes(&self, id: DownloadId, total_bytes: u64) {
        self.emit(DownloadEvent::VerifiedMapSizes { id, total_bytes });
    }

    pub fn low_disk_space(&self, id: DownloadId, available_bytes: u64) {
        self.emit(DownloadEvent::LowDiskSpace { id, available_bytes });
    }
}

impl Default for StatusSink {
    fn default() -> Self {
        Self::noop()
    }
}

#[derive(Clone)]
pub struct DownloadContext {
    pub id: DownloadId,
    pub thread_count: usize,
    pub skip_existing: bool,
    pub auto_overwrite: bool,
    pub verify_zip_eocd: bool,
    pub shutdown: ShutdownToken,
    pub client: reqwest::Client,
    pub mirror_pool: MirrorPool,
    pub output_dir: Arc<PathBuf>,
    pub tracker: BeatmapTracker,
    pub cleanup_tracker: CleanupTracker,
    pub initial_unverified: Arc<DashSet<u32>>,
    pub status: StatusSink,
    pub progress_watchdog: Duration,
}

impl DownloadContext {
    pub fn new(config: DownloadContextConfig) -> Self {
        let DownloadContextConfig {
            id,
            thread_count,
            skip_existing,
            auto_overwrite,
            verify_zip_eocd,
            shutdown,
            client,
            mirror_pool,
            output_dir,
            tracker,
            initial_unverified,
            status,
            progress_watchdog,
        } = config;

        Self {
            id,
            thread_count,
            skip_existing,
            auto_overwrite,
            verify_zip_eocd,
            shutdown,
            client,
            mirror_pool,
            output_dir: Arc::new(output_dir),
            tracker,
            cleanup_tracker: CleanupTracker::new(),
            initial_unverified,
            status,
            progress_watchdog,
        }
    }

    pub fn emit(&self, event: DownloadEvent) {
        self.status.emit(event);
    }

    pub fn status_sink(&self) -> StatusSink {
        self.status.clone()
    }

    pub fn consume_unverified(&self, beatmapset_id: u32) -> bool {
        self.initial_unverified.remove(&beatmapset_id).is_some()
    }
}
