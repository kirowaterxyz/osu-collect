//! Worker module for streaming downloads and I/O

use crate::{validation::HashWorker, DownloadError, Result};
use futures_util::StreamExt;
use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{fs, io::AsyncWriteExt, time::timeout};

/// Minimum bytes changed to emit progress update
const MIN_PROGRESS_DELTA: u64 = 131_072; // 128 KB

/// Minimum interval between progress updates
const MIN_PROGRESS_INTERVAL: Duration = Duration::from_millis(200);

/// Result of a streaming download operation
pub struct DownloadStreamResult {
    /// Whether the download was cancelled
    pub cancelled: bool,
    /// MD5 hash of the downloaded file (if computed)
    pub hash: Option<String>,
    /// Total bytes written
    pub bytes_written: u64,
}

/// Download a response stream to a file with progress tracking
///
/// This function:
/// - Streams the response body to disk
/// - Computes MD5 hash in the background
/// - Calls progress callback periodically
/// - Handles cancellation and timeouts
/// - Cleans up on error
pub async fn download_with_streaming(
    response: reqwest::Response,
    output_path: &Path,
    content_length: Option<u64>,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    progress_timeout: Duration,
    mut cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> Result<DownloadStreamResult> {
    let mut file = fs::File::create(output_path).await?;
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let total = content_length.unwrap_or(0);
    let mut hash_worker = Some(HashWorker::new());

    let mut last_progress_bytes = 0u64;
    let mut last_progress_emitted = Instant::now();

    loop {
        // Check for cancellation
        if *cancel_rx.borrow_and_update() {
            if let Some(worker) = hash_worker.take() {
                worker.abort();
            }
            file.shutdown().await.ok();
            let _ = fs::remove_file(output_path).await;
            return Ok(DownloadStreamResult {
                cancelled: true,
                hash: None,
                bytes_written: downloaded,
            });
        }

        // Get next chunk with timeout
        let maybe_chunk = match timeout(progress_timeout, stream.next()).await {
            Ok(chunk) => chunk,
            Err(_) => {
                if let Some(worker) = hash_worker.take() {
                    worker.abort();
                }
                file.shutdown().await.ok();
                let _ = fs::remove_file(output_path).await;
                return Err(DownloadError::ProgressTimeout.into());
            }
        };

        let Some(chunk) = maybe_chunk else {
            break;
        };

        let chunk = match chunk {
            Ok(bytes) => bytes,
            Err(err) => {
                if let Some(worker) = hash_worker.take() {
                    worker.abort();
                }
                file.shutdown().await.ok();
                let _ = fs::remove_file(output_path).await;
                return Err(DownloadError::http(err.to_string()).into());
            }
        };

        downloaded += chunk.len() as u64;

        // Write to file
        if let Err(err) = file.write_all(&chunk).await {
            if let Some(worker) = hash_worker.take() {
                worker.abort();
            }
            file.shutdown().await.ok();
            let _ = fs::remove_file(output_path).await;
            return Err(DownloadError::io(err.to_string()).into());
        }

        // Update hash
        if let Some(worker) = hash_worker.as_ref() {
            worker.update(chunk);
        }

        // Emit progress
        if let Some(ref callback) = progress_callback {
            let delta = downloaded.saturating_sub(last_progress_bytes);
            if delta >= MIN_PROGRESS_DELTA
                || last_progress_emitted.elapsed() >= MIN_PROGRESS_INTERVAL
            {
                callback(downloaded, total);
                last_progress_bytes = downloaded;
                last_progress_emitted = Instant::now();
            }
        }
    }

    // Final cancellation check
    if *cancel_rx.borrow() {
        if let Some(worker) = hash_worker.take() {
            worker.abort();
        }
        file.shutdown().await.ok();
        let _ = fs::remove_file(output_path).await;
        return Ok(DownloadStreamResult {
            cancelled: true,
            hash: None,
            bytes_written: downloaded,
        });
    }

    // Flush and finalize
    if let Err(err) = file.flush().await {
        if let Some(worker) = hash_worker.take() {
            worker.abort();
        }
        if let Err(rm_err) = fs::remove_file(output_path).await {
            tracing::warn!(path = %output_path.display(), error = %rm_err, "failed to remove partial file after flush error");
        }
        return Err(DownloadError::io(err.to_string()).into());
    }

    if let Err(err) = file.sync_data().await {
        if let Some(worker) = hash_worker.take() {
            worker.abort();
        }
        if let Err(rm_err) = fs::remove_file(output_path).await {
            tracing::warn!(path = %output_path.display(), error = %rm_err, "failed to remove partial file after sync error");
        }
        return Err(DownloadError::io(err.to_string()).into());
    }

    if let Err(err) = file.shutdown().await {
        if let Some(worker) = hash_worker.take() {
            worker.abort();
        }
        if let Err(rm_err) = fs::remove_file(output_path).await {
            tracing::warn!(path = %output_path.display(), error = %rm_err, "failed to remove partial file after shutdown error");
        }
        return Err(DownloadError::io(err.to_string()).into());
    }

    // Finalize hash
    let hash = if let Some(worker) = hash_worker.take() {
        Some(worker.finalize().await?)
    } else {
        None
    };

    // Final progress callback
    if let Some(ref callback) = progress_callback {
        callback(downloaded, total);
    }

    Ok(DownloadStreamResult {
        cancelled: false,
        hash,
        bytes_written: downloaded,
    })
}
