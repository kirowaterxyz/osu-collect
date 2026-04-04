use super::{DownloadEvent, DownloadId, DownloadStage, DownloadSummary};
use crate::worker::StatusSink;

pub(crate) fn log_status(status: &StatusSink, id: DownloadId, message: impl Into<String>) {
    status.emit(DownloadEvent::Log {
        id,
        message: message.into(),
    });
}

pub(crate) fn stage_status(status: &StatusSink, id: DownloadId, stage: DownloadStage) {
    status.emit(DownloadEvent::StageChanged { id, stage });
}

pub(crate) fn fail_status(status: &StatusSink, id: DownloadId, message: impl Into<String>) {
    status.emit(DownloadEvent::Failed {
        id,
        message: message.into(),
    });
}

pub(crate) fn finished_status(status: &StatusSink, id: DownloadId, summary: &DownloadSummary) {
    status.emit(DownloadEvent::Finished {
        id,
        summary: summary.clone(),
    });
}

pub(crate) fn target_status(status: &StatusSink, id: DownloadId, remaining: usize) {
    status.emit(DownloadEvent::DownloadTarget { id, remaining });
}

pub(crate) fn progress_status(status: &StatusSink, id: DownloadId, summary: &DownloadSummary) {
    status.emit(DownloadEvent::OverallProgress {
        id,
        downloaded: summary.downloaded,
        skipped: summary.skipped,
        failed: summary.failed,
        unverified: summary.unverified,
    });
}

pub(crate) fn verified_sizes_status(status: &StatusSink, id: DownloadId, total_bytes: u64) {
    status.emit(DownloadEvent::VerifiedMapSizes { id, total_bytes });
}

pub(crate) fn low_disk_space_status(status: &StatusSink, id: DownloadId, available_bytes: u64) {
    status.emit(DownloadEvent::LowDiskSpace {
        id,
        available_bytes,
    });
}
