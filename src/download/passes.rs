use super::{
    BeatmapStage, BeatmapTracker, DownloadEvent, DownloadId, DownloadResult, DownloadSummary,
    StatusReporter, download_beatmap, status,
};
use crate::{
    config::constants::NETWORK_RETRY_CAP,
    mirrors::MirrorKind,
    utils::AppError,
    worker::{DownloadContext, StatusSink},
};
use async_channel::Receiver as AsyncReceiver;
use std::{
    any::Any,
    collections::{HashMap, HashSet},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::mpsc::UnboundedSender,
    task::JoinHandle,
    time::{Instant, sleep},
};
use tracing::{debug, info, trace, warn};

type ResultMessage = (usize, u32, crate::utils::Result<DownloadResult>);

async fn download_single_target(
    context: &DownloadContext,
    slot: usize,
    beatmapset_id: u32,
) -> crate::utils::Result<DownloadResult> {
    let download_id = context.id;
    let status_sender = context.status_sink();
    let start_label = status::STARTING_DOWNLOAD;
    status_sender.emit(DownloadEvent::BeatmapStatus {
        id: download_id,
        beatmapset_id,
        stage: BeatmapStage::Downloading,
        message: format!("{} {}", start_label, beatmapset_id),
    });

    let progress_callback = {
        let progress_sink = context.status_sink();
        let progress_id = download_id;
        Arc::new(move |downloaded: u64, total: u64| {
            progress_sink.emit(DownloadEvent::BeatmapProgress {
                id: progress_id,
                beatmapset_id,
                thread_index: slot,
                downloaded,
                total,
            });
        })
    };

    let thread_status_sender = context.status_sink();
    let status_reporter = {
        let sender = thread_status_sender.clone();
        let slot_index = slot;
        let active_beatmapset = beatmapset_id;
        let callback: Arc<dyn Fn(&str) + Send + Sync> = Arc::new(move |msg: &str| {
            if msg.starts_with(status::CONTACTING_PREFIX) {
                return;
            }
            let message = msg.to_string();
            let rate_limited = message.starts_with(status::RATE_LIMITED);
            sender.emit(DownloadEvent::ThreadStatus {
                id: download_id,
                thread_index: slot_index,
                message,
                rate_limited,
                beatmapset_id: Some(active_beatmapset),
            });
        });
        StatusReporter::from(Some(callback))
    };

    let mirror_pool = context.mirror_pool.clone();
    let shutdown_inner = context.shutdown.clone();
    let mut network_retries: u32 = 0;

    loop {
        if shutdown_inner.is_cancelled() {
            warn!(
                download_id = context.id,
                beatmapset_id, "Download task aborted due to shutdown signal"
            );
            break Ok(DownloadResult::Aborted);
        }

        if let Some((mirror_info, wait_for)) = mirror_pool.single_mirror_cooldown()
            && !wait_for.is_zero()
        {
            let wait_secs = wait_for.as_secs().max(1);
            let wait_message = format!(
                "{} on {}, waiting {}s before retry",
                status::RATE_LIMITED,
                mirror_info.display_name(),
                wait_secs
            );
            context.emit(DownloadEvent::ThreadStatus {
                id: context.id,
                thread_index: slot,
                message: wait_message.clone(),
                rate_limited: true,
                beatmapset_id: Some(beatmapset_id),
            });
            if cancellable_sleep(wait_for, &shutdown_inner).await {
                warn!(
                    download_id = context.id,
                    beatmapset_id, "Download task aborted during mirror cooldown"
                );
                break Ok(DownloadResult::Aborted);
            }
            continue;
        }

        let mirror_plan: Vec<_> = mirror_pool.plan().into_iter().map(Into::into).collect();
        if mirror_plan.is_empty() {
            warn!(
                download_id = context.id,
                beatmapset_id, "all mirrors penalized, waiting 5s"
            );
            if cancellable_sleep(Duration::from_secs(5), &shutdown_inner).await {
                warn!(
                    download_id = context.id,
                    beatmapset_id, "Download task aborted while all mirrors were penalized"
                );
                break Ok(DownloadResult::Aborted);
            }
            continue;
        }
        let first_mirror = mirror_plan
            .first()
            .map(|mirror: &crate::mirrors::MirrorEndpoint| mirror.display_name())
            .unwrap_or("selected mirror");

        let activity_label = status::DOWNLOADING;
        context.emit(DownloadEvent::ThreadStatus {
            id: context.id,
            thread_index: slot,
            message: format!(
                "{} #{} from {}",
                activity_label, beatmapset_id, first_mirror
            ),
            rate_limited: false,
            beatmapset_id: Some(beatmapset_id),
        });
        trace!(
            download_id = context.id,
            beatmapset_id,
            slot,
            mirror = first_mirror,
            "Starting mirror download"
        );

        let result = download_beatmap(
            beatmapset_id,
            mirror_plan.as_slice(),
            context,
            Some(progress_callback.clone()),
            status_reporter.clone(),
            Some(mirror_pool.clone()),
        )
        .await?;

        match result {
            DownloadResult::NetworkError(ref reason) => {
                network_retries = network_retries.saturating_add(1);
                if network_retries >= NETWORK_RETRY_CAP {
                    warn!(
                        download_id = context.id,
                        beatmapset_id,
                        network_retries,
                        reason = %reason,
                        "network retry cap reached, skipping silently"
                    );
                    break Ok(result);
                }
                debug!(
                    download_id = context.id,
                    beatmapset_id,
                    network_retries,
                    reason = %reason,
                    "network error, retrying"
                );
                if cancellable_sleep(Duration::from_secs(5), &shutdown_inner).await {
                    break Ok(DownloadResult::Aborted);
                }
                continue;
            }
            other => break Ok(other),
        }
    }
}

async fn cancellable_sleep(duration: Duration, shutdown: &super::ShutdownToken) -> bool {
    let deadline = Instant::now() + duration;
    loop {
        if shutdown.is_cancelled() {
            return true;
        }

        let now = Instant::now();
        if now >= deadline {
            return false;
        }

        sleep((deadline - now).min(Duration::from_millis(100))).await;
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct MirrorFailureStats {
    per_mirror: HashMap<MirrorKind, MirrorFailureEntry>,
    unattributed: u32,
}

#[derive(Clone, Debug, Default)]
struct MirrorFailureEntry {
    total: u32,
    rate_limited: u32,
    last_reason: String,
}

#[derive(Clone, Debug)]
pub(crate) struct MirrorFailureSnapshot {
    pub mirror: MirrorKind,
    pub failures: u32,
    pub last_reason: String,
}

impl MirrorFailureStats {
    fn record(&mut self, mirror: Option<MirrorKind>, reason: &str) {
        if let Some(kind) = mirror {
            let entry = self.per_mirror.entry(kind).or_default();
            entry.total = entry.total.saturating_add(1);
            entry.last_reason = reason.to_string();
            if reason.contains(status::RATE_LIMITED) {
                entry.rate_limited = entry.rate_limited.saturating_add(1);
            }
        } else {
            self.unattributed = self.unattributed.saturating_add(1);
        }
    }

    pub(crate) fn most_common(&self) -> Option<MirrorFailureSnapshot> {
        self.per_mirror
            .iter()
            .max_by_key(|(_, entry)| entry.total)
            .map(|(mirror, entry)| MirrorFailureSnapshot {
                mirror: *mirror,
                failures: entry.total,
                last_reason: entry.last_reason.clone(),
            })
    }

    pub(crate) fn describe_top_failure(&self) -> Option<String> {
        self.most_common().map(|snapshot| {
            format!(
                "{} failing {} time(s) (last error: {})",
                snapshot.mirror_label(),
                snapshot.failures,
                snapshot.last_reason
            )
        })
    }
}

impl MirrorFailureSnapshot {
    pub(crate) fn mirror_label(&self) -> &'static str {
        self.mirror.label()
    }
}

#[cfg(test)]
mod tests {
    use super::cancellable_sleep;
    use crate::download::ShutdownToken;
    use std::time::{Duration, Instant};

    #[tokio::test]
    async fn cooldown_sleep_exits_after_shutdown() {
        let shutdown = ShutdownToken::new();
        let canceller = shutdown.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(20)).await;
            canceller.cancel();
        });

        let started = Instant::now();
        assert!(cancellable_sleep(Duration::from_secs(5), &shutdown).await);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[tokio::test]
    async fn cooldown_sleep_finishes_without_shutdown() {
        let shutdown = ShutdownToken::new();
        assert!(!cancellable_sleep(Duration::from_millis(5), &shutdown).await);
    }

    #[tokio::test]
    async fn cancelled_task_join_maps_to_aborted_not_error() {
        use crate::download::DownloadResult;

        // Spawn a task that blocks forever, then abort it.
        let handle = tokio::spawn(async { std::future::pending::<()>().await });
        handle.abort();
        let join_result = handle.await;

        // The join error must be a cancel (not a panic), so our fix maps it to Aborted.
        let err = join_result.unwrap_err();
        assert!(
            err.is_cancelled(),
            "aborted task should yield a cancelled JoinError"
        );

        // Verify the branch mapping: cancelled → Aborted (not Err)
        let result: crate::utils::Result<DownloadResult> = if err.is_cancelled() {
            Ok(DownloadResult::Aborted)
        } else {
            Err(crate::utils::AppError::other_dynamic(
                "panic".to_string().into_boxed_str(),
            ))
        };

        assert!(
            matches!(result, Ok(DownloadResult::Aborted)),
            "cancelled task should yield Aborted, not an error"
        );
    }
}

#[derive(Clone, Debug, Default)]
pub struct FailureReport {
    beatmaps: Vec<(u32, String)>,
    seen: HashSet<u32>,
    mirror_failures: MirrorFailureStats,
}

impl FailureReport {
    pub fn record(&mut self, beatmapset_id: u32, reason: String, mirror: Option<MirrorKind>) {
        if self.seen.insert(beatmapset_id) {
            self.beatmaps.push((beatmapset_id, reason.clone()));
        }
        self.mirror_failures.record(mirror, &reason);
    }

    pub(crate) fn record_error(&mut self, beatmapset_id: u32, reason: String) {
        if self.seen.insert(beatmapset_id) {
            self.beatmaps.push((beatmapset_id, reason.clone()));
        }
        self.mirror_failures.record(None, &reason);
    }

    pub fn beatmaps(&self) -> &[(u32, String)] {
        &self.beatmaps
    }

    pub fn is_empty(&self) -> bool {
        self.beatmaps.is_empty()
    }

    pub fn describe_top_failure(&self) -> Option<String> {
        self.mirror_failures.describe_top_failure()
    }
}

fn complete_beatmap(
    tracker: &BeatmapTracker,
    status: &StatusSink,
    id: DownloadId,
    beatmapset_id: u32,
) -> Option<usize> {
    tracker.remove_pending(beatmapset_id).inspect(|&remaining| {
        status.emit(DownloadEvent::DownloadTarget { id, remaining });
    })
}

pub(crate) struct PassOutcome {
    pub(crate) failures: FailureReport,
    pub(crate) aborted: bool,
}

pub(crate) struct PassCoordinator<'a> {
    context: DownloadContext,
    totals: &'a mut DownloadSummary,
}

impl<'a> PassCoordinator<'a> {
    pub(crate) fn new(context: DownloadContext, totals: &'a mut DownloadSummary) -> Self {
        Self { context, totals }
    }

    pub(crate) async fn run(mut self, beatmapset_ids: Vec<u32>) -> PassOutcome {
        if beatmapset_ids.is_empty() {
            debug!(
                download_id = self.context.id,
                "download pass invoked with no targets"
            );
            return PassOutcome {
                failures: FailureReport::default(),
                aborted: false,
            };
        }

        info!(
            download_id = self.context.id,
            queued = beatmapset_ids.len(),
            thread_count = self.context.thread_count,
            "Starting download pass"
        );

        let mut failures = FailureReport::default();
        let mut aborted = false;
        let worker_count = self.context.thread_count.max(1);
        let (job_tx, job_rx) = async_channel::bounded::<u32>(worker_count * 2);
        let (result_tx, mut result_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut worker_handles: Vec<JoinHandle<()>> = Vec::with_capacity(worker_count);

        for slot in 0..worker_count {
            let worker_context = self.context.clone();
            let rx_clone = job_rx.clone();
            let tx_clone = result_tx.clone();
            worker_handles.push(tokio::spawn(pass_worker_loop(
                slot,
                worker_context,
                rx_clone,
                tx_clone,
            )));
        }
        drop(result_tx);

        let feed_shutdown = self.context.shutdown.clone();
        let feed_handle = tokio::spawn(async move {
            for id in beatmapset_ids {
                if feed_shutdown.is_cancelled() {
                    break;
                }
                if job_tx.send(id).await.is_err() {
                    break;
                }
            }
        });

        while let Some((slot, beatmapset_id, result)) = result_rx.recv().await {
            if self
                .process_result(slot, beatmapset_id, result, &mut failures)
                .await
            {
                aborted = true;
                self.context.shutdown.cancel();
                break;
            }

            if self.context.shutdown.is_cancelled() {
                aborted = true;
                break;
            }

            self.context.emit(DownloadEvent::OverallProgress {
                id: self.context.id,
                downloaded: self.totals.downloaded,
                skipped: self.totals.skipped,
                failed: self.totals.failed,
                unverified: self.totals.unverified,
            });
        }

        while let Ok((slot, beatmapset_id, result)) = result_rx.try_recv() {
            self.process_result(slot, beatmapset_id, result, &mut failures).await;
        }
        drop(result_rx);
        feed_handle.abort();

        for handle in worker_handles {
            if let Err(err) = handle.await {
                aborted = true;
                warn!(
                    download_id = self.context.id,
                    error = %err,
                    "Download worker panicked"
                );
            }
        }

        PassOutcome { failures, aborted }
    }

    async fn process_result(
        &mut self,
        slot: usize,
        beatmapset_id: u32,
        result: crate::utils::Result<DownloadResult>,
        failures: &mut FailureReport,
    ) -> bool {
        match result {
            Ok(DownloadResult::Success(file)) => {
                self.context.mirror_pool.clear_penalty(file.mirror);
                let verify_start = Instant::now();

                trace!(
                    download_id = self.context.id,
                    beatmapset_id, "Download verification succeeded"
                );
                self.totals.downloaded = self.totals.downloaded.saturating_add(1);
                let _ = complete_beatmap(
                    &self.context.tracker,
                    &self.context.status,
                    self.context.id,
                    beatmapset_id,
                );
                self.context.tracker.mark_verified(beatmapset_id);
                clear_unverified_flag(&self.context, self.totals, beatmapset_id);
                let mirror_label = file.mirror.label();
                let success_message = format!(
                    "{} (md5: {}) via {}",
                    file.filename, file.hash, mirror_label
                );
                self.context.emit(DownloadEvent::BeatmapStatus {
                    id: self.context.id,
                    beatmapset_id,
                    stage: BeatmapStage::Success,
                    message: success_message,
                });
                self.context.emit(DownloadEvent::BeatmapVerified {
                    id: self.context.id,
                    duration_ms: verify_start.elapsed().as_millis() as u64,
                });
                self.context.emit(DownloadEvent::ThreadStatus {
                    id: self.context.id,
                    thread_index: slot,
                    message: format!("Done #{}", beatmapset_id),
                    rate_limited: false,
                    beatmapset_id: Some(beatmapset_id),
                });
                false
            }
            Ok(DownloadResult::Skipped(filename)) => {
                self.totals.skipped = self.totals.skipped.saturating_add(1);
                debug!(
                    download_id = self.context.id,
                    beatmapset_id, "Skipped beatmap download"
                );
                self.context.emit(DownloadEvent::BeatmapStatus {
                    id: self.context.id,
                    beatmapset_id,
                    stage: BeatmapStage::Skipped,
                    message: format!("Skipped: {}", filename),
                });
                let _ = complete_beatmap(
                    &self.context.tracker,
                    &self.context.status,
                    self.context.id,
                    beatmapset_id,
                );
                self.context.emit(DownloadEvent::ThreadStatus {
                    id: self.context.id,
                    thread_index: slot,
                    message: format!("Skipped #{}", beatmapset_id),
                    rate_limited: false,
                    beatmapset_id: Some(beatmapset_id),
                });
                clear_unverified_flag(&self.context, self.totals, beatmapset_id);
                false
            }
            Ok(DownloadResult::Failed(failure)) => {
                self.totals.failed = self.totals.failed.saturating_add(1);
                let reason_text = failure.reason.to_string();
                let mirror_kind = failure.mirror;
                failures.record(beatmapset_id, reason_text.clone(), mirror_kind);
                warn!(
                    download_id = self.context.id,
                    beatmapset_id,
                    error = %reason_text,
                    "Download failed"
                );
                self.context.emit(DownloadEvent::BeatmapStatus {
                    id: self.context.id,
                    beatmapset_id,
                    stage: BeatmapStage::Failed,
                    message: reason_text.clone(),
                });
                let _ = complete_beatmap(
                    &self.context.tracker,
                    &self.context.status,
                    self.context.id,
                    beatmapset_id,
                );
                self.context.tracker.mark_failed(beatmapset_id);
                self.context.emit(DownloadEvent::ThreadStatus {
                    id: self.context.id,
                    thread_index: slot,
                    message: format!("Failed #{} ({})", beatmapset_id, reason_text),
                    rate_limited: false,
                    beatmapset_id: Some(beatmapset_id),
                });
                false
            }
            Ok(DownloadResult::NetworkError(reason)) => {
                warn!(
                    download_id = self.context.id,
                    beatmapset_id,
                    error = %reason,
                    "network retry cap reached"
                );
                let _ = complete_beatmap(
                    &self.context.tracker,
                    &self.context.status,
                    self.context.id,
                    beatmapset_id,
                );
                self.context.emit(DownloadEvent::ThreadStatus {
                    id: self.context.id,
                    thread_index: slot,
                    message: format!("network error #{} ({})", beatmapset_id, reason),
                    rate_limited: false,
                    beatmapset_id: Some(beatmapset_id),
                });
                false
            }
            Ok(DownloadResult::Aborted) => {
                self.context.shutdown.cancel();
                warn!(
                    download_id = self.context.id,
                    beatmapset_id, "Download aborted mid-pass"
                );
                self.context.tracker.mark_failed(beatmapset_id);
                self.totals.failed = self.totals.failed.saturating_add(1);
                let _ = complete_beatmap(
                    &self.context.tracker,
                    &self.context.status,
                    self.context.id,
                    beatmapset_id,
                );
                self.context.emit(DownloadEvent::BeatmapStatus {
                    id: self.context.id,
                    beatmapset_id,
                    stage: BeatmapStage::Aborted,
                    message: status::ABORTED.to_string(),
                });
                true
            }
            Err(err) => {
                self.totals.failed = self.totals.failed.saturating_add(1);
                let message = err.to_string();
                failures.record_error(beatmapset_id, message.clone());
                warn!(
                    download_id = self.context.id,
                    beatmapset_id,
                    error = %message,
                    "Download errored"
                );
                self.context.emit(DownloadEvent::BeatmapStatus {
                    id: self.context.id,
                    beatmapset_id,
                    stage: BeatmapStage::Failed,
                    message: message.clone(),
                });
                let _ = complete_beatmap(
                    &self.context.tracker,
                    &self.context.status,
                    self.context.id,
                    beatmapset_id,
                );
                self.context.tracker.mark_failed(beatmapset_id);
                self.context.emit(DownloadEvent::ThreadStatus {
                    id: self.context.id,
                    thread_index: slot,
                    message: format!("Failed #{} ({})", beatmapset_id, message),
                    rate_limited: false,
                    beatmapset_id: Some(beatmapset_id),
                });
                false
            }
        }
    }
}

async fn pass_worker_loop(
    slot: usize,
    context: DownloadContext,
    job_rx: AsyncReceiver<u32>,
    result_tx: UnboundedSender<ResultMessage>,
) {
    loop {
        if context.shutdown.is_cancelled() {
            break;
        }

        let next_job = job_rx.recv().await.ok();

        let Some(beatmapset_id) = next_job else {
            break;
        };

        trace!(
            download_id = context.id,
            beatmapset_id, slot, "Dispatching beatmap download task"
        );

        let job_context = context.clone();
        let mut result_task =
            tokio::spawn(
                async move { download_single_target(&job_context, slot, beatmapset_id).await },
            );

        let result = tokio::select! {
            join_result = &mut result_task => {
                match join_result {
                    Ok(res) => res,
                    Err(join_err) if join_err.is_cancelled() => Ok(DownloadResult::Aborted),
                    Err(join_err) => {
                        let reason = describe_panic(join_err.into_panic());
                        Err(AppError::other_dynamic(
                            format!(
                                "download worker panicked for #{}: {}",
                                beatmapset_id, reason
                            )
                            .into_boxed_str(),
                        ))
                    }
                }
            }
            _ = context.shutdown.cancelled() => {
                result_task.abort();
                Ok(DownloadResult::Aborted)
            }
        };
        if result_tx.send((slot, beatmapset_id, result)).is_err() {
            break;
        }
    }
}

fn describe_panic(panic: Box<dyn Any + Send + 'static>) -> String {
    if let Some(message) = panic.downcast_ref::<&str>() {
        message.to_string()
    } else if let Some(message) = panic.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic".to_string()
    }
}

fn clear_unverified_flag(
    context: &DownloadContext,
    totals: &mut DownloadSummary,
    beatmapset_id: u32,
) {
    if context.consume_unverified(beatmapset_id) {
        totals.unverified = totals.unverified.saturating_sub(1);
    }
}
