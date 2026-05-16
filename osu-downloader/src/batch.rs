//! Batch download orchestration.
//!
//! The library entrypoint ([`Downloader::download_many`](crate::Downloader::download_many))
//! delegates here. We feed a bounded queue from the caller's items and run a worker pool of
//! `concurrent_downloads` tasks that pull from it.

use crate::{
    download::{
        self, download_beatmapset, BeatmapsetDownloadCallbacks, BeatmapsetDownloadOptions,
        BeatmapsetDownloadOutcome,
    },
    downloader::{BeatmapsetStatusEvent, DownloadItem},
    mirrors::MirrorPool,
    DownloadEvent, DownloadSummary, SkipReason,
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
    pub(crate) verify_archives: bool,
    pub(crate) progress_timeout: Duration,
}

pub(crate) async fn download_batch(
    items: Vec<DownloadItem>,
    output_dir: &Path,
    client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
    config: BatchConfig,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    cancel_rx: watch::Receiver<bool>,
) -> DownloadSummary {
    let start_time = Instant::now();
    let total = items.len();
    let mut summary = DownloadSummary::new(total);
    let _ = event_tx.send(DownloadEvent::SessionStarted {
        total_beatmapsets: total,
    });

    if items.is_empty() {
        finalize(summary, &event_tx, start_time);
        return summary_dummy_returned();
    }

    let worker_count = config.concurrent_downloads.max(1);
    let (job_tx, job_rx) = async_bounded_channel(worker_count * 2);
    let feed_cancel = cancel_rx.clone();
    let feed_handle = tokio::spawn(async move {
        let cancel = feed_cancel;
        for item in items {
            if *cancel.borrow() {
                break;
            }
            if job_tx.send(item).await.is_err() {
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
            DownloadOutcome::NetworkError { beatmapset_id, .. } => {
                summary.network_errors.push(beatmapset_id)
            }
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
        network_errors = summary.network_errors.len(),
        total,
        "batch complete"
    );
    summary
}

fn finalize(
    mut summary: DownloadSummary,
    event_tx: &mpsc::UnboundedSender<DownloadEvent>,
    start_time: Instant,
) {
    summary.duration = start_time.elapsed();
    let _ = event_tx.send(DownloadEvent::SessionCompleted { summary });
}

fn summary_dummy_returned() -> DownloadSummary {
    DownloadSummary::new(0)
}

enum DownloadOutcome {
    Success {
        beatmapset_id: u32,
        size_bytes: u64,
    },
    Skipped {
        beatmapset_id: u32,
        reason: SkipReason,
    },
    Failed {
        beatmapset_id: u32,
        error: String,
    },
    NetworkError {
        beatmapset_id: u32,
    },
    Aborted,
}

#[allow(clippy::too_many_arguments)]
async fn worker_loop(
    job_rx: AsyncBoundedReceiver<DownloadItem>,
    result_tx: mpsc::UnboundedSender<DownloadOutcome>,
    client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
    config: BatchConfig,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    cancel_rx: watch::Receiver<bool>,
    output_dir: std::path::PathBuf,
) {
    loop {
        if *cancel_rx.borrow() {
            break;
        }
        let Ok(item) = job_rx.recv().await else {
            break;
        };
        let outcome = process_one(
            item,
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
    item: DownloadItem,
    output_dir: &Path,
    client: &reqwest::Client,
    mirror_pool: &MirrorPool,
    config: &BatchConfig,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    cancel_rx: watch::Receiver<bool>,
) -> DownloadOutcome {
    let beatmapset_id = item.beatmapset_id;
    debug!(beatmapset_id, "starting download");

    let _ = event_tx.send(DownloadEvent::BeatmapsetStarted { beatmapset_id });

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
        let _ = event_tx_progress.send(DownloadEvent::Progress {
            beatmapset_id,
            downloaded_bytes: downloaded,
            total_bytes: if total > 0 { Some(total) } else { None },
            speed_bps,
        });
    });

    let event_tx_status = event_tx.clone();
    let status_callback = Arc::new(move |status: BeatmapsetStatusEvent| {
        let _ = event_tx_status.send(DownloadEvent::BeatmapsetStatus {
            beatmapset_id,
            status,
        });
    });

    let (outcome, _retries) = download_beatmapset(download::DownloadParams {
        beatmapset_id,
        output_dir,
        client,
        mirror_pool,
        verify_archive: config.verify_archives,
        progress_timeout: config.progress_timeout,
        callbacks: BeatmapsetDownloadCallbacks {
            progress: Some(progress_callback),
            status: Some(status_callback),
        },
        options: BeatmapsetDownloadOptions {
            file_exists_policy: item.policy,
        },
        cancel_rx,
    })
    .await;

    match outcome {
        BeatmapsetDownloadOutcome::Success {
            filename,
            hash,
            mirror,
            size_bytes,
            verify_duration_us,
        } => {
            let _ = event_tx.send(DownloadEvent::BeatmapsetCompleted {
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
            let _ = event_tx.send(DownloadEvent::BeatmapsetSkipped {
                beatmapset_id,
                reason: reason.clone(),
            });
            DownloadOutcome::Skipped {
                beatmapset_id,
                reason,
            }
        }
        BeatmapsetDownloadOutcome::Failed { mirror, reason } => {
            let _ = event_tx.send(DownloadEvent::BeatmapsetFailed {
                beatmapset_id,
                error: crate::DownloadError::worker_error(reason.clone()),
                mirror,
            });
            DownloadOutcome::Failed {
                beatmapset_id,
                error: reason,
            }
        }
        BeatmapsetDownloadOutcome::NetworkError { reason } => {
            let _ = event_tx.send(DownloadEvent::BeatmapsetNetworkError {
                beatmapset_id,
                reason,
            });
            DownloadOutcome::NetworkError { beatmapset_id }
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
mod tests {
    use super::*;
    use crate::Mirror;

    #[tokio::test]
    async fn cancel_mid_batch_does_not_panic() {
        let dir = tempfile::tempdir().unwrap();
        let (event_tx, _event_rx) = mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let client = reqwest::Client::new();
        let mirror_pool = Arc::new(MirrorPool::new(vec![Mirror::nerinyan()]));
        let config = BatchConfig {
            concurrent_downloads: 2,
            verify_archives: false,
            progress_timeout: Duration::from_secs(1),
        };

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let _ = cancel_tx.send(true);
        });

        let items: Vec<DownloadItem> = (1u32..=5).map(DownloadItem::skip_if_present).collect();
        let summary = download_batch(
            items,
            dir.path(),
            client,
            mirror_pool,
            config,
            event_tx,
            cancel_rx,
        )
        .await;

        assert!(
            summary.downloaded.len()
                + summary.skipped.len()
                + summary.failed.len()
                + summary.network_errors.len()
                <= 5
        );
    }
}
