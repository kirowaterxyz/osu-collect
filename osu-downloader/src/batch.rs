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
    let mut tasks = Vec::new();

    for beatmapset_id in beatmapset_ids {
        let permit = semaphore.clone().acquire_owned().await.unwrap();
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
            (beatmapset_id, result)
        });

        tasks.push(task);
    }

    // Wait for all tasks to complete
    for task in tasks {
        if let Ok((beatmapset_id, result)) = task.await {
            match result {
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

    // Get first mirror for event
    let mirrors = mirror_pool.plan();
    if let Some(first_mirror) = mirrors.first() {
        let _ = event_tx.send(DownloadEvent::BeatmapsetStarted {
            beatmapset_id,
            mirror: first_mirror.kind(),
        });
    }

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
    let result = download_beatmapset(download::DownloadParams {
        beatmapset_id,
        output_dir,
        client,
        mirror_pool,
        verify_archive: config.verify_archives,
        progress_timeout: config.progress_timeout,
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
                retry_count: 0,
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduplicate_ids_preserves_first_occurrence_order() {
        assert_eq!(deduplicate_ids(vec![3, 1, 3, 2, 1]), vec![3, 1, 2]);
    }
}
