use super::{BeatmapStage, DownloadEvent, DownloadId, DownloadStage, DownloadSummary, Emit};
use crate::config::constants::status;
use osu_downloader::{BeatmapsetStatusEvent, DownloadEvent as LibEvent, SkipReason};
use std::collections::HashSet;
use tracing::warn;

/// Running counters for a pipeline run. Consumed by `translate_event` and converted
/// into the app-facing `DownloadSummary` at the end of a run.
#[derive(Default)]
pub struct Tally {
    pub downloaded: u32,
    pub skipped: u32,
    pub failed: u32,
    pub unverified: u32,
    pub failures: Vec<(u32, String)>,
    /// Beatmapset IDs that this run downloaded successfully.
    pub successful: HashSet<u32>,
}

impl Tally {
    pub fn to_summary(&self) -> DownloadSummary {
        DownloadSummary {
            downloaded: self.downloaded,
            skipped: self.skipped,
            failed: self.failed,
            unverified: self.unverified,
        }
    }

    fn record_completed(&mut self, beatmapset_id: u32) {
        self.downloaded = self.downloaded.saturating_add(1);
        self.successful.insert(beatmapset_id);
        if self.unverified > 0 {
            self.unverified = self.unverified.saturating_sub(1);
        }
    }

    fn record_skipped(&mut self) {
        self.skipped = self.skipped.saturating_add(1);
    }

    fn record_failed(&mut self, beatmapset_id: u32, reason: String) {
        self.failed = self.failed.saturating_add(1);
        self.failures.push((beatmapset_id, reason));
    }
}

pub fn translate_event(id: DownloadId, event: LibEvent, tally: &mut Tally, emit: Emit<'_>) {
    match event {
        LibEvent::SessionStarted { total_beatmapsets } => emit(DownloadEvent::Log {
            id,
            message: format!("downloading {total_beatmapsets} beatmapsets"),
        }),
        LibEvent::BeatmapsetStarted { .. } | LibEvent::SessionCompleted { .. } => {}
        LibEvent::BeatmapsetStatus {
            beatmapset_id,
            status,
        } => emit_status(id, beatmapset_id, status, emit),
        LibEvent::Progress {
            beatmapset_id,
            downloaded_bytes,
            total_bytes,
            ..
        } => emit(DownloadEvent::BeatmapProgress {
            id,
            beatmapset_id,
            downloaded: downloaded_bytes,
            total: total_bytes.unwrap_or(0),
        }),
        LibEvent::BeatmapsetCompleted {
            beatmapset_id,
            mirror_used,
            verify_duration_us,
            ..
        } => {
            tally.record_completed(beatmapset_id);
            emit_terminal_status(
                id,
                beatmapset_id,
                BeatmapStage::Success,
                format!("downloaded from {}", mirror_used.label()),
                emit,
            );
            emit(DownloadEvent::BeatmapVerified {
                id,
                duration_us: verify_duration_us,
            });
            emit_overall_progress(id, tally, emit);
        }
        LibEvent::BeatmapsetSkipped {
            beatmapset_id,
            reason,
        } => match reason {
            SkipReason::AlreadyExists => {
                tally.record_skipped();
                emit_terminal_status(
                    id,
                    beatmapset_id,
                    BeatmapStage::Skipped,
                    "skipped: already exists".to_string(),
                    emit,
                );
                emit_overall_progress(id, tally, emit);
            }
            SkipReason::UnavailableOnMirrors | SkipReason::InvalidBeatmapsetId => {
                let message = match reason {
                    SkipReason::UnavailableOnMirrors => "unavailable on all mirrors",
                    SkipReason::InvalidBeatmapsetId => "invalid beatmapset id",
                    SkipReason::AlreadyExists => unreachable!(),
                }
                .to_string();
                record_and_emit_failed(id, beatmapset_id, message, tally, emit);
            }
        },
        LibEvent::BeatmapsetFailed {
            beatmapset_id,
            error,
            ..
        } => record_and_emit_failed(id, beatmapset_id, error.to_string(), tally, emit),
        LibEvent::BeatmapsetNetworkError {
            beatmapset_id,
            reason,
        } => {
            warn!(beatmapset_id, %reason, "network error, all mirrors exhausted");
            record_and_emit_failed(
                id,
                beatmapset_id,
                format!("network error: {reason}"),
                tally,
                emit,
            );
        }
    }
}

fn record_and_emit_failed(
    id: DownloadId,
    beatmapset_id: u32,
    message: String,
    tally: &mut Tally,
    emit: Emit<'_>,
) {
    tally.record_failed(beatmapset_id, message.clone());
    emit_terminal_status(id, beatmapset_id, BeatmapStage::Failed, message, emit);
    emit_overall_progress(id, tally, emit);
}

fn emit_terminal_status(
    id: DownloadId,
    beatmapset_id: u32,
    stage: BeatmapStage,
    message: String,
    emit: Emit<'_>,
) {
    emit(DownloadEvent::BeatmapStatus {
        id,
        beatmapset_id,
        stage,
        message,
        rate_limited: false,
    });
}

fn emit_status(id: DownloadId, beatmapset_id: u32, event: BeatmapsetStatusEvent, emit: Emit<'_>) {
    let (message, stage, rate_limited) = match event {
        // dont remove this
        BeatmapsetStatusEvent::Contacting { mirror } => (
            format!("checking {}", mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
        BeatmapsetStatusEvent::Downloading { mirror } => (
            format!("{} from {}", status::DOWNLOADING, mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
        BeatmapsetStatusEvent::Verifying { mirror } => (
            format!("verifying from {}", mirror.label()),
            BeatmapStage::Verifying,
            false,
        ),
        BeatmapsetStatusEvent::RateLimited { mirror, cooldown } => (
            format!(
                "{} on {}, waiting {}s",
                status::RATE_LIMITED,
                mirror.label(),
                cooldown.as_secs().max(1)
            ),
            BeatmapStage::Downloading,
            true,
        ),
        BeatmapsetStatusEvent::RetryingTransient {
            mirror,
            attempt,
            max_attempts,
            reason,
        } => (
            format!(
                "retrying {} after {reason} (attempt {attempt}/{max_attempts})",
                mirror.label()
            ),
            BeatmapStage::Downloading,
            false,
        ),
        BeatmapsetStatusEvent::MirrorFailed { mirror, reason } => (
            format!("{} failed: {reason}", mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
    };
    emit(DownloadEvent::BeatmapStatus {
        id,
        beatmapset_id,
        stage,
        message,
        rate_limited,
    });
}

pub fn emit_overall_progress(id: DownloadId, tally: &Tally, emit: Emit<'_>) {
    emit(DownloadEvent::OverallProgress {
        id,
        downloaded: tally.downloaded,
        skipped: tally.skipped,
        failed: tally.failed,
        unverified: tally.unverified,
    });
}

pub fn emit_finish(id: DownloadId, emit: Emit<'_>, summary: DownloadSummary) {
    emit(DownloadEvent::Finished { id, summary });
    emit(DownloadEvent::StageChanged {
        id,
        stage: DownloadStage::Completed,
    });
}
