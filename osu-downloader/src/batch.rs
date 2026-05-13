//! Batch download orchestration

use crate::{
    download::{self, download_beatmapset},
    mirrors::MirrorPool,
    DownloadEvent, DownloadResult, DownloadSummary,
};
use std::{
    collections::HashSet,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::mpsc;
use tracing::{debug, info};

/// Orchestrate batch downloads with concurrent workers
pub async fn download_batch(
    beatmapset_ids: Vec<u32>,
    output_dir: &Path,
    client: reqwest::Client,
    mirror_pool: Arc<MirrorPool>,
    config: BatchConfig,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> DownloadSummary {
    let start_time = Instant::now();
    let beatmapset_ids = deduplicate_ids(beatmapset_ids);
    let total = beatmapset_ids.len();

    // Send session started event
    let _ = event_tx.send(DownloadEvent::SessionStarted {
        total_beatmapsets: total,
    });

    let mut summary = DownloadSummary::new(total);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(config.concurrent_downloads));
    let mut tasks: Vec<(
        u32,
        tokio::task::JoinHandle<Result<DownloadResult, crate::Error>>,
    )> = Vec::new();

    for beatmapset_id in beatmapset_ids {
        if *cancel_rx.borrow() {
            break;
        }

        let permit = semaphore
            .clone()
            .acquire_owned()
            .await
            .expect("download semaphore closed unexpectedly");
        let client = client.clone();
        let mirror_pool = mirror_pool.clone();
        let output_dir = output_dir.to_path_buf();
        let event_tx = event_tx.clone();
        let cancel_rx = cancel_rx.clone();
        let config = config.clone();

        let task = tokio::spawn(async move {
            let result = download_single_with_events(
                beatmapset_id,
                &output_dir,
                &client,
                &mirror_pool,
                &config,
                event_tx,
                cancel_rx,
            )
            .await;

            drop(permit);
            result
        });

        tasks.push((beatmapset_id, task));
    }

    // Wait for all tasks to complete
    for (beatmapset_id, task) in tasks {
        match task.await {
            Ok(result) => match result {
                Ok(DownloadResult::Success { size_bytes, .. }) => {
                    summary.downloaded.push(beatmapset_id);
                    summary.total_bytes += size_bytes;
                }
                Ok(DownloadResult::Skipped { reason }) => {
                    summary.skipped.push((beatmapset_id, reason));
                }
                Err(e) => {
                    summary.failed.push((beatmapset_id, e.to_string()));
                }
            },
            Err(join_err) => {
                let msg = if join_err.is_panic() {
                    format!("task panicked: {join_err}")
                } else {
                    format!("task cancelled: {join_err}")
                };
                let _ = event_tx.send(DownloadEvent::BeatmapsetFailed {
                    beatmapset_id,
                    error: crate::DownloadError::worker_error(&msg),
                    retry_count: 0,
                });
                summary.failed.push((beatmapset_id, msg));
            }
        }
    }

    summary.duration = start_time.elapsed();

    // Send session completed event
    let _ = event_tx.send(DownloadEvent::SessionCompleted {
        summary: summary.clone(),
    });

    info!(
        "Batch download complete: {}/{} downloaded, {} skipped, {} failed",
        summary.downloaded.len(),
        total,
        summary.skipped.len(),
        summary.failed.len()
    );

    summary
}

fn deduplicate_ids(ids: Vec<u32>) -> Vec<u32> {
    let mut seen = HashSet::with_capacity(ids.len());
    ids.into_iter().filter(|id| seen.insert(*id)).collect()
}

/// Download a single beatmapset and emit events
async fn download_single_with_events(
    beatmapset_id: u32,
    output_dir: &Path,
    client: &reqwest::Client,
    mirror_pool: &MirrorPool,
    config: &BatchConfig,
    event_tx: mpsc::UnboundedSender<DownloadEvent>,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<DownloadResult, crate::Error> {
    debug!("Starting download of beatmapset {}", beatmapset_id);

    let _ = event_tx.send(DownloadEvent::BeatmapsetStarted {
        beatmapset_id,
        mirror: mirror_pool.plan().first().map(|m| m.kind()).unwrap_or(crate::MirrorKind::Custom),
    });

    // Progress callback with speed calculation
    let event_tx_clone = event_tx.clone();
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

        let _ = event_tx_clone.send(DownloadEvent::Progress {
            beatmapset_id,
            downloaded_bytes: downloaded,
            total_bytes: if total > 0 { Some(total) } else { None },
            speed_bps,
        });
    });

    // Attempt download
    let (result, retry_count) = download_beatmapset(download::DownloadParams {
        beatmapset_id,
        output_dir,
        client,
        mirror_pool,
        verify_archive: config.verify_archives,
        progress_timeout: config.progress_timeout,
        max_retries: config.max_retries,
        progress_callback: Some(progress_callback),
        cancel_rx,
    })
    .await;

    // Send completion event
    match &result {
        Ok(DownloadResult::Success {
            filename,
            size_bytes,
            md5_hash,
            mirror_used,
        }) => {
            let _ = event_tx.send(DownloadEvent::BeatmapsetCompleted {
                beatmapset_id,
                filename: filename.clone(),
                size_bytes: *size_bytes,
                md5_hash: md5_hash.clone(),
                mirror_used: *mirror_used,
            });
        }
        Ok(DownloadResult::Skipped { reason }) => {
            let _ = event_tx.send(DownloadEvent::BeatmapsetSkipped {
                beatmapset_id,
                reason: reason.clone(),
            });
        }
        Err(e) => {
            let _ = event_tx.send(DownloadEvent::BeatmapsetFailed {
                beatmapset_id,
                error: crate::DownloadError::worker_error(e.to_string()),
                retry_count,
            });
        }
    }

    result
}

/// Configuration for batch downloads
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Number of concurrent downloads
    pub concurrent_downloads: usize,
    /// Whether to verify archives
    pub verify_archives: bool,
    /// Progress timeout
    pub progress_timeout: Duration,
    /// maximum retry attempts per mirror for transient failures
    pub max_retries: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicate_ids_preserves_first_occurrence_order() {
        assert_eq!(deduplicate_ids(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }

    #[tokio::test]
    async fn started_event_precedes_completed_and_failed() {
        use crate::{Mirror, MirrorPool};

        let dir = tempfile::tempdir().unwrap();
        let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();
        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let client = reqwest::Client::new();
        let mirror_pool = Arc::new(MirrorPool::new(vec![Mirror::nerinyan()]));
        let config = BatchConfig {
            concurrent_downloads: 1,
            verify_archives: false,
            progress_timeout: std::time::Duration::from_secs(5),
            max_retries: 0,
        };

        // We only want to check ordering, not actual download success.
        // Run with cancel immediately so the task aborts quickly.
        drop(cancel_tx);
        let _ = cancel_rx; // to reuse with channel

        let (cancel_tx2, cancel_rx2) = tokio::sync::watch::channel(false);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            let _ = cancel_tx2.send(true);
        });

        let _ = download_single_with_events(
            999_999_999,
            dir.path(),
            &client,
            &mirror_pool,
            &config,
            event_tx.clone(),
            cancel_rx2,
        )
        .await;

        drop(event_tx);
        let mut events = Vec::new();
        while let Some(ev) = event_rx.recv().await {
            events.push(ev);
        }

        let started_pos = events.iter().position(|e| {
            matches!(e, DownloadEvent::BeatmapsetStarted { .. })
        });
        let ended_pos = events.iter().position(|e| {
            matches!(
                e,
                DownloadEvent::BeatmapsetCompleted { .. }
                    | DownloadEvent::BeatmapsetFailed { .. }
                    | DownloadEvent::BeatmapsetSkipped { .. }
            )
        });

        assert!(
            started_pos.is_some(),
            "BeatmapsetStarted should always be emitted"
        );
        if let Some(end_idx) = ended_pos {
            assert!(
                started_pos.unwrap() < end_idx,
                "Started must precede Completed/Failed/Skipped"
            );
        }
    }
}
