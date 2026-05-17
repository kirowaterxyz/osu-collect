pub mod error;
pub mod lock;
mod pipeline;
mod precheck;
mod session;

pub use error::DownloadError;
pub use lock::ActiveDownloadRegistry;
pub use pipeline::{spawn_download, spawn_selective_download};

pub use crate::config::constants::status;
pub use osu_downloader::ArchiveValidation;

use crate::mirrors::Mirror;
use tokio::{sync::watch, task::JoinHandle};

pub type DownloadId = u64;

/// Handle to a running download task.
pub struct DownloadHandle {
    cancel: watch::Sender<bool>,
    join: JoinHandle<()>,
}

impl DownloadHandle {
    pub(crate) fn new(cancel: watch::Sender<bool>, join: JoinHandle<()>) -> Self {
        Self { cancel, join }
    }

    pub fn request_shutdown(&self) {
        let _ = self.cancel.send(true);
    }

    pub async fn wait(self) {
        let _ = self.join.await;
    }
}

#[derive(Debug, Clone)]
pub struct DownloadConfig {
    pub directory: String,
    pub mirrors: Vec<Mirror>,
    pub concurrent: u8,
    pub archive_validation: ArchiveValidation,
}

#[derive(Debug, Clone)]
pub struct DownloadRequest {
    pub collection_input: String,
    pub config: DownloadConfig,
    pub auto_overwrite: bool,
}

#[derive(Debug, Clone)]
pub struct SelectiveDownloadCollection {
    pub id: u32,
    pub name: String,
    pub beatmapset_ids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct SelectiveDownloadRequest {
    pub collection_ids: Vec<u32>,
    pub beatmapset_ids: Vec<u32>,
    pub collections: Vec<SelectiveDownloadCollection>,
    pub config: DownloadConfig,
    pub snapshot_dir: Option<std::path::PathBuf>,
    pub snapshots: Vec<crate::app::snapshots::CollectionSnapshotFile>,
}

#[derive(Debug, Clone)]
pub enum DownloadEvent {
    CollectionReady {
        id: DownloadId,
        collection_name: String,
        uploader: String,
        total_maps: usize,
        output_dir: String,
    },
    ResolveProgress {
        id: DownloadId,
        current: u32,
        total: u32,
    },
    CollectionSizeResolved {
        id: DownloadId,
        total_bytes: u64,
    },
    LowDiskSpace {
        id: DownloadId,
        available_bytes: u64,
    },
    VerifiedMapSizes {
        id: DownloadId,
        total_bytes: u64,
    },
    BeatmapsRegistered {
        id: DownloadId,
        beatmap_ids: Vec<u32>,
    },
    BeatmapProgress {
        id: DownloadId,
        beatmapset_id: u32,
        downloaded: u64,
        total: u64,
    },
    DownloadTarget {
        id: DownloadId,
        remaining: usize,
    },
    BeatmapStatus {
        id: DownloadId,
        beatmapset_id: u32,
        stage: BeatmapStage,
        message: String,
        rate_limited: bool,
    },
    OverallProgress {
        id: DownloadId,
        downloaded: u32,
        skipped: u32,
        failed: u32,
        unverified: u32,
    },
    Log {
        id: DownloadId,
        message: String,
    },
    StageChanged {
        id: DownloadId,
        stage: DownloadStage,
    },
    FailedMaps {
        id: DownloadId,
        failures: Vec<(u32, String)>,
    },
    BeatmapVerified {
        id: DownloadId,
        duration_us: u64,
    },
    Finished {
        id: DownloadId,
        summary: DownloadSummary,
    },
    Failed {
        id: DownloadId,
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BeatmapStage {
    Pending,
    Downloading,
    /// archive bytes done; lib is hashing/zip-validating/finalizing before emitting a terminal stage.
    Verifying,
    Success,
    Skipped,
    Failed,
    Aborted,
}

impl BeatmapStage {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Success | Self::Skipped | Self::Failed | Self::Aborted
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownloadStage {
    Pending,
    Resolving,
    Rechecking,
    Downloading,
    Completed,
    Failed,
}

#[derive(Debug, Clone)]
pub struct DownloadSummary {
    pub downloaded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub unverified: u32,
}
