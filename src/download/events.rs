use super::{BeatmapStage, DownloadEvent, DownloadId, DownloadStage, DownloadSummary, Emit};
use crate::config::constants::status::{
    self, CHECKING_PREFIX, DOWNLOADED_FROM_PREFIX, FROM_SUFFIX, RATE_LIMITED_SUFFIX,
    RETRYING_AFTER, RETRYING_ATTEMPT_PREFIX, RETRYING_PREFIX, VERIFYING_PREFIX,
};
use osu_downloader::{Error as LibError, Event as LibEvent, Skip, Status};
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
        LibEvent::SessionStarted { total } => emit(DownloadEvent::Log {
            id,
            message: format!("downloading {total} beatmapsets"),
        }),
        LibEvent::SessionCompleted { .. } => {}
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
            let label = mirror_used.label();
            let mut msg = String::with_capacity(DOWNLOADED_FROM_PREFIX.len() + label.len());
            msg.push_str(DOWNLOADED_FROM_PREFIX);
            msg.push_str(label);
            emit_terminal_status(id, beatmapset_id, BeatmapStage::Success, msg, emit);
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
            Skip::AlreadyExists => {
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
            Skip::UnavailableOnMirrors => {
                record_and_emit_failed(
                    id,
                    beatmapset_id,
                    "unavailable on all mirrors".to_string(),
                    tally,
                    emit,
                );
            }
        },
        LibEvent::BeatmapsetFailed {
            beatmapset_id,
            error,
            ..
        } => {
            if error.is_transient() {
                let reason = match &error {
                    LibError::Network(msg) => msg.clone(),
                    other => other.to_string(),
                };
                warn!(beatmapset_id, %reason, "network error, all mirrors exhausted");
                record_and_emit_failed(
                    id,
                    beatmapset_id,
                    format!("network error: {reason}"),
                    tally,
                    emit,
                );
            } else {
                record_and_emit_failed(id, beatmapset_id, error.to_string(), tally, emit);
            }
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

fn status_msg(prefix: &str, label: &str) -> String {
    let mut s = String::with_capacity(prefix.len() + label.len());
    s.push_str(prefix);
    s.push_str(label);
    s
}

fn emit_status(id: DownloadId, beatmapset_id: u32, event: Status, emit: Emit<'_>) {
    let (message, stage, rate_limited) = match event {
        // dont remove this
        Status::Contacting { mirror } => (
            status_msg(CHECKING_PREFIX, mirror.label()),
            BeatmapStage::Downloading,
            false,
        ),
        Status::Downloading { mirror } => {
            let label = mirror.label();
            let mut s =
                String::with_capacity(status::DOWNLOADING.len() + FROM_SUFFIX.len() + label.len());
            s.push_str(status::DOWNLOADING);
            s.push_str(FROM_SUFFIX);
            s.push_str(label);
            (s, BeatmapStage::Downloading, false)
        }
        Status::Verifying { mirror } => (
            status_msg(VERIFYING_PREFIX, mirror.label()),
            BeatmapStage::Verifying,
            false,
        ),
        Status::RateLimited { cooldown } => {
            let secs = cooldown.as_secs().max(1).to_string();
            let mut s = String::with_capacity(
                status::RATE_LIMITED.len() + RATE_LIMITED_SUFFIX.len() + secs.len() + 1,
            );
            s.push_str(status::RATE_LIMITED);
            s.push_str(RATE_LIMITED_SUFFIX);
            s.push_str(&secs);
            s.push('s');
            (s, BeatmapStage::Downloading, true)
        }
        Status::RetryingTransient {
            mirror,
            attempt,
            max_attempts,
            reason,
        } => {
            let label = mirror.label();
            let attempt_s = attempt.to_string();
            let max_s = max_attempts.to_string();
            let mut s = String::with_capacity(
                RETRYING_PREFIX.len()
                    + label.len()
                    + RETRYING_AFTER.len()
                    + reason.len()
                    + RETRYING_ATTEMPT_PREFIX.len()
                    + attempt_s.len()
                    + 1
                    + max_s.len()
                    + 1,
            );
            s.push_str(RETRYING_PREFIX);
            s.push_str(label);
            s.push_str(RETRYING_AFTER);
            s.push_str(&reason);
            s.push_str(RETRYING_ATTEMPT_PREFIX);
            s.push_str(&attempt_s);
            s.push('/');
            s.push_str(&max_s);
            s.push(')');
            (s, BeatmapStage::Downloading, false)
        }
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
