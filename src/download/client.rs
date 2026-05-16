use crate::{
    config::constants::status,
    download::{BeatmapStage, DownloadEvent},
    mirrors::{Mirror, MirrorKind},
    utils::{AppError, Result},
    worker::DownloadContext,
};
use osu_downloader::{
    BeatmapsetDownloadCallbacks, BeatmapsetDownloadOptions, BeatmapsetDownloadOutcome,
    BeatmapsetStatusEvent, Downloader, FileExistsPolicy, SkipReason,
};
use std::{borrow::Cow, sync::Arc};

pub type StatusCallback = Arc<dyn Fn(&str) + Send + Sync>;

fn emit_status(reporter: &Option<StatusCallback>, message: impl AsRef<str>) {
    if let Some(callback) = reporter {
        callback(message.as_ref());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadResult {
    Success(CompletedDownload),
    Skipped(Box<str>),
    Failed(DownloadFailure),
    NetworkError(String),
    Aborted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadFailure {
    pub mirror: Option<MirrorKind>,
    pub reason: Cow<'static, str>,
}

impl DownloadResult {
    fn failed(mirror: Option<MirrorKind>, reason: impl Into<Cow<'static, str>>) -> Self {
        DownloadResult::Failed(DownloadFailure {
            mirror,
            reason: reason.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedDownload {
    pub filename: Box<str>,
    pub hash: Box<str>,
    pub mirror: MirrorKind,
    pub verify_duration_us: u64,
}

pub fn create_download_client() -> Result<reqwest::Client> {
    osu_downloader::http::create_download_client(None).map_err(|err| {
        AppError::other_dynamic(format!("failed to create download client: {err}").into_boxed_str())
    })
}

pub async fn download_beatmap(
    beatmapset_id: u32,
    mirrors: &[Mirror],
    context: &DownloadContext,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    status_reporter: Option<StatusCallback>,
) -> Result<DownloadResult> {
    if context.shutdown.is_cancelled() {
        return Ok(DownloadResult::Aborted);
    }

    if mirrors.is_empty() {
        return Ok(DownloadResult::failed(None, "all mirrors failed"));
    }

    let callbacks = BeatmapsetDownloadCallbacks {
        progress: progress_callback,
        status: Some(status_callback(context, beatmapset_id, status_reporter)),
    };
    let cancel_rx = cancellation_channel(context);
    let downloader = Downloader::builder()
        .mirrors(mirrors.iter().cloned())
        .concurrent_downloads(context.thread_count.max(1))
        .verify_archives(context.verify_zip_eocd)
        .progress_timeout(context.progress_watchdog)
        .build()
        .map_err(|err| AppError::other_dynamic(err.to_string().into_boxed_str()))?;

    let options = BeatmapsetDownloadOptions {
        file_exists_policy: if context.auto_overwrite {
            FileExistsPolicy::OverwriteTarget
        } else {
            FileExistsPolicy::Skip
        },
    };
    let outcome = downloader
        .download_beatmapset_with_options(
            beatmapset_id,
            mirrors,
            context.output_dir.as_ref(),
            callbacks,
            options,
            cancel_rx,
        )
        .await;

    Ok(outcome.into())
}

fn status_callback(
    context: &DownloadContext,
    beatmapset_id: u32,
    reporter: Option<StatusCallback>,
) -> Arc<dyn Fn(BeatmapsetStatusEvent) + Send + Sync> {
    let sink = context.status_sink();
    let download_id = context.id;
    Arc::new(move |event| {
        let (message, stage) = status_update(beatmapset_id, event);
        if let Some(stage) = stage {
            sink.emit(DownloadEvent::BeatmapStatus {
                id: download_id,
                beatmapset_id,
                stage,
                message: message.clone(),
            });
        }
        emit_status(&reporter, message_for_thread(&message, beatmapset_id));
    })
}

fn message_for_thread(message: &str, beatmapset_id: u32) -> String {
    if message.contains(&format!("#{}", beatmapset_id)) {
        message.to_string()
    } else {
        format!("{} #{}", message, beatmapset_id)
    }
}

fn status_update(
    beatmapset_id: u32,
    event: BeatmapsetStatusEvent,
) -> (String, Option<BeatmapStage>) {
    match event {
        BeatmapsetStatusEvent::Contacting { mirror } => (
            format!(
                "{} #{} from {}",
                status::CONTACTING_PREFIX,
                beatmapset_id,
                mirror.label()
            ),
            Some(BeatmapStage::Downloading),
        ),
        BeatmapsetStatusEvent::Downloading { mirror } => (
            format!(
                "{} #{} from {}",
                status::DOWNLOADING,
                beatmapset_id,
                mirror.label()
            ),
            Some(BeatmapStage::Downloading),
        ),
        BeatmapsetStatusEvent::Verifying { mirror } => (
            format!("Verifying #{} from {}", beatmapset_id, mirror.label()),
            Some(BeatmapStage::Downloading),
        ),
        BeatmapsetStatusEvent::RateLimited { mirror, cooldown } => {
            let wait_secs = cooldown.as_secs().max(1);
            (
                format!(
                    "{} on {}, waiting {}s before retry",
                    status::RATE_LIMITED,
                    mirror.label(),
                    wait_secs
                ),
                Some(BeatmapStage::Downloading),
            )
        }
        BeatmapsetStatusEvent::RetryingTransient {
            mirror,
            attempt,
            max_attempts,
            reason,
        } => (
            format!(
                "Retrying {} after {} (attempt {}/{})",
                mirror.label(),
                reason,
                attempt,
                max_attempts
            ),
            Some(BeatmapStage::Downloading),
        ),
        BeatmapsetStatusEvent::MirrorFailed { mirror, reason } => {
            (format!("{} failed: {}", mirror.label(), reason), None)
        }
    }
}

fn cancellation_channel(context: &DownloadContext) -> tokio::sync::watch::Receiver<bool> {
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(context.shutdown.is_cancelled());
    if !context.shutdown.is_cancelled() {
        let shutdown = context.shutdown.clone();
        tokio::spawn(async move {
            shutdown.cancelled().await;
            let _ = cancel_tx.send(true);
        });
    }
    cancel_rx
}

impl From<BeatmapsetDownloadOutcome> for DownloadResult {
    fn from(outcome: BeatmapsetDownloadOutcome) -> Self {
        match outcome {
            BeatmapsetDownloadOutcome::Success {
                filename,
                hash,
                mirror,
                size_bytes: _,
                verify_duration_us,
            } => DownloadResult::Success(CompletedDownload {
                filename: filename.into_boxed_str(),
                hash: hash.into_boxed_str(),
                mirror,
                verify_duration_us,
            }),
            BeatmapsetDownloadOutcome::Skipped { reason } => {
                DownloadResult::Skipped(skipped_reason(reason).into_boxed_str())
            }
            BeatmapsetDownloadOutcome::Failed { mirror, reason } => {
                DownloadResult::failed(mirror, reason)
            }
            BeatmapsetDownloadOutcome::NetworkError { reason } => {
                DownloadResult::NetworkError(reason)
            }
            BeatmapsetDownloadOutcome::Aborted => DownloadResult::Aborted,
        }
    }
}

fn skipped_reason(reason: SkipReason) -> String {
    match reason {
        SkipReason::AlreadyExists => "already exists".to_string(),
        SkipReason::UnavailableOnMirrors => "unavailable on all mirrors".to_string(),
        SkipReason::InvalidBeatmapsetId => "invalid beatmapset id".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn verifying_status_updates_beatmap_without_failing_it() {
        let (message, stage) = status_update(
            123,
            BeatmapsetStatusEvent::Verifying {
                mirror: MirrorKind::Nerinyan,
            },
        );

        assert_eq!(stage, Some(BeatmapStage::Downloading));
        assert!(message.contains("Verifying #123"));
    }

    #[test]
    fn rate_limit_status_stays_in_downloading_stage() {
        let (message, stage) = status_update(
            123,
            BeatmapsetStatusEvent::RateLimited {
                mirror: MirrorKind::Nerinyan,
                cooldown: Duration::from_secs(2),
            },
        );

        assert_eq!(stage, Some(BeatmapStage::Downloading));
        assert!(message.contains("Rate limited"));
    }

    #[test]
    fn mirror_failed_status_does_not_mark_beatmap_failed() {
        let (_, stage) = status_update(
            123,
            BeatmapsetStatusEvent::MirrorFailed {
                mirror: MirrorKind::Nerinyan,
                reason: "HTTP 500".to_string(),
            },
        );

        assert_eq!(stage, None);
    }
}
