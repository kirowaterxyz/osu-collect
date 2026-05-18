//! Batch download orchestration.
//!
//! The library entrypoint ([`Downloader::download_many`](crate::Downloader::download_many))
//! delegates here. We feed a bounded queue from the caller's items and run a worker pool of
//! `concurrent_downloads` tasks that pull from it.

use crate::{
    Error, Event, Summary,
    config::NETWORK_RETRY_BACKOFF,
    download::{self, BeatmapsetDownloadCallbacks, BeatmapsetDownloadOutcome, download_beatmapset},
    event::{Skip, Status},
    downloader::OnExists,
    mirrors::MirrorPool,
    validation::ArchiveValidation,
};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, watch};
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub(crate) struct BatchConfig {
    pub(crate) concurrent_downloads: usize,
    pub(crate) archive_validation: ArchiveValidation,
    pub(crate) progress_timeout: Duration,
    pub(crate) network_retry_attempts: usize,
    pub(crate) sanitize_filenames: bool,
    pub(crate) on_exists: OnExists,
}

pub(crate) async fn download_batch(
    ids: Vec<u32>,
    output_dir: &Path,
    client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
    config: BatchConfig,
    event_tx: mpsc::UnboundedSender<Event>,
    cancel_rx: watch::Receiver<bool>,
) -> Summary {
    let start_time = Instant::now();
    let total = ids.len();
    let mut summary = Summary::new(total);
    let _ = event_tx.send(Event::SessionStarted { total });

    if ids.is_empty() {
        finalize(summary, &event_tx, start_time);
        return Summary::new(0);
    }

    let worker_count = config.concurrent_downloads.max(1);
    let (job_tx, job_rx) = async_bounded_channel(worker_count * 2);
    let feed_cancel = cancel_rx.clone();
    let feed_handle = tokio::spawn(async move {
        let cancel = feed_cancel;
        for id in ids {
            if *cancel.borrow() {
                break;
            }
            if job_tx.send(id).await.is_err() {
                break;
            }
        }
    });

    let (result_tx, mut result_rx) = mpsc::unbounded_channel::<DownloadOutcome>();
    let mut worker_handles = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let job_rx = job_rx.clone();
        let result_tx = result_tx.clone();
        let client = client.clone();
        let mirror_pool = mirror_pool.clone();
        let config = config.clone();
        let event_tx = event_tx.clone();
        let cancel_rx = cancel_rx.clone();
        let output_dir = output_dir.to_path_buf();
        worker_handles.push(tokio::spawn(async move {
            worker_loop(
                job_rx,
                result_tx,
                client,
                mirror_pool,
                config,
                event_tx,
                cancel_rx,
                output_dir,
            )
            .await;
        }));
    }
    drop(result_tx);

    while let Some(outcome) = result_rx.recv().await {
        match outcome {
            DownloadOutcome::Success {
                beatmapset_id,
                size_bytes,
            } => {
                summary.downloaded.push(beatmapset_id);
                summary.total_bytes += size_bytes;
            }
            DownloadOutcome::Skipped {
                beatmapset_id,
                reason,
            } => summary.skipped.push((beatmapset_id, reason)),
            DownloadOutcome::Failed {
                beatmapset_id,
                error,
            } => summary.failed.push((beatmapset_id, error)),
            DownloadOutcome::Aborted => {}
        }
    }

    feed_handle.abort();
    for handle in worker_handles {
        let _ = handle.await;
    }

    finalize(summary.clone(), &event_tx, start_time);

    summary.duration = start_time.elapsed();
    info!(
        downloaded = summary.downloaded.len(),
        skipped = summary.skipped.len(),
        failed = summary.failed.len(),
        total,
        "batch complete"
    );
    summary
}

fn finalize(mut summary: Summary, event_tx: &mpsc::UnboundedSender<Event>, start_time: Instant) {
    summary.duration = start_time.elapsed();
    let _ = event_tx.send(Event::SessionCompleted { summary });
}

enum DownloadOutcome {
    Success {
        beatmapset_id: u32,
        size_bytes: u64,
    },
    Skipped {
        beatmapset_id: u32,
        reason: Skip,
    },
    Failed {
        beatmapset_id: u32,
        error: Error,
    },
    Aborted,
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    job_rx: AsyncBoundedReceiver<u32>,
    result_tx: mpsc::UnboundedSender<DownloadOutcome>,
    client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
    config: BatchConfig,
    event_tx: mpsc::UnboundedSender<Event>,
    cancel_rx: watch::Receiver<bool>,
    output_dir: std::path::PathBuf,
) {
    loop {
        if *cancel_rx.borrow() {
            break;
        }
        let Ok(beatmapset_id) = job_rx.recv().await else {
            break;
        };
        let outcome = process_one(
            beatmapset_id,
            &output_dir,
            &client,
            &mirror_pool,
            &config,
            event_tx.clone(),
            cancel_rx.clone(),
        )
        .await;
        if result_tx.send(outcome).is_err() {
            break;
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn process_one(
    beatmapset_id: u32,
    output_dir: &Path,
    client: &reqwest::Client,
    mirror_pool: &MirrorPool,
    config: &BatchConfig,
    event_tx: mpsc::UnboundedSender<Event>,
    cancel_rx: watch::Receiver<bool>,
) -> DownloadOutcome {
    debug!(beatmapset_id, "starting download");

    let event_tx_progress = event_tx.clone();
    let progress_state = Arc::new(Mutex::new((0u64, Instant::now())));
    let progress_callback = Arc::new(move |downloaded: u64, total: u64| {
        let speed_bps = {
            let mut state = progress_state.lock().unwrap();
            let (last_bytes, last_time) = *state;
            let now = Instant::now();
            let elapsed = now.duration_since(last_time).as_secs_f64();
            let speed = if elapsed > 0.0 && downloaded > last_bytes {
                ((downloaded - last_bytes) as f64 / elapsed) as u64
            } else {
                0
            };
            *state = (downloaded, now);
            speed
        };
        let _ = event_tx_progress.send(Event::Progress {
            beatmapset_id,
            downloaded_bytes: downloaded,
            total_bytes: if total > 0 { Some(total) } else { None },
            speed_bps,
        });
    });

    let event_tx_status = event_tx.clone();
    let status_callback = Arc::new(move |status: Status| {
        let _ = event_tx_status.send(Event::BeatmapsetStatus {
            beatmapset_id,
            status,
        });
    });

    let mut outcome;
    let mut attempts_remaining = config.network_retry_attempts;
    loop {
        outcome = download_beatmapset(download::DownloadParams {
            beatmapset_id,
            output_dir,
            client,
            mirror_pool,
            archive_validation: config.archive_validation,
            progress_timeout: config.progress_timeout,
            sanitize_filenames: config.sanitize_filenames,
            on_exists: config.on_exists,
            callbacks: BeatmapsetDownloadCallbacks {
                progress: Some(progress_callback.clone()),
                status: Some(status_callback.clone()),
            },
            cancel_rx: cancel_rx.clone(),
        })
        .await
        .0;

        if !matches!(outcome, BeatmapsetDownloadOutcome::NetworkError { .. })
            || attempts_remaining == 0
            || *cancel_rx.borrow()
        {
            break;
        }

        attempts_remaining -= 1;
        let cancelled = tokio::select! {
            _ = tokio::time::sleep(NETWORK_RETRY_BACKOFF) => false,
            changed = async {
                let mut rx = cancel_rx.clone();
                rx.changed().await
            } => changed.is_err() || *cancel_rx.borrow(),
        };
        if cancelled {
            break;
        }
    }

    match outcome {
        BeatmapsetDownloadOutcome::Success {
            filename,
            hash,
            mirror,
            size_bytes,
            verify_duration_us,
        } => {
            let _ = event_tx.send(Event::BeatmapsetCompleted {
                beatmapset_id,
                filename,
                size_bytes,
                md5_hash: Some(hash),
                mirror_used: mirror,
                verify_duration_us,
            });
            DownloadOutcome::Success {
                beatmapset_id,
                size_bytes,
            }
        }
        BeatmapsetDownloadOutcome::Skipped { reason } => {
            let _ = event_tx.send(Event::BeatmapsetSkipped {
                beatmapset_id,
                reason: reason.clone(),
            });
            DownloadOutcome::Skipped {
                beatmapset_id,
                reason,
            }
        }
        BeatmapsetDownloadOutcome::Failed { mirror, reason } => {
            let error = Error::validation(reason);
            let _ = event_tx.send(Event::BeatmapsetFailed {
                beatmapset_id,
                error: error.clone(),
                mirror,
            });
            DownloadOutcome::Failed {
                beatmapset_id,
                error,
            }
        }
        BeatmapsetDownloadOutcome::NetworkError { reason } => {
            let error = Error::network(reason);
            let _ = event_tx.send(Event::BeatmapsetFailed {
                beatmapset_id,
                error: error.clone(),
                mirror: None,
            });
            DownloadOutcome::Failed {
                beatmapset_id,
                error,
            }
        }
        BeatmapsetDownloadOutcome::Aborted => {
            warn!(beatmapset_id, "download aborted");
            DownloadOutcome::Aborted
        }
    }
}

// Thin wrapper around `async_channel` so the call sites stay tidy.
type AsyncBoundedSender<T> = async_channel::Sender<T>;
type AsyncBoundedReceiver<T> = async_channel::Receiver<T>;

fn async_bounded_channel<T>(capacity: usize) -> (AsyncBoundedSender<T>, AsyncBoundedReceiver<T>) {
    async_channel::bounded(capacity)
}

#[cfg(test)]
#[path = "../tests/batch.rs"]
mod tests;
