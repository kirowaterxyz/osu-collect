use crate::{
    config::constants::{
        EOCD_SIGNATURE, MAX_EOCD_SEARCH_BYTES, MIN_PROGRESS_DELTA, MIN_PROGRESS_INTERVAL,
    },
    download::ShutdownToken,
    utils::{AppError, Result},
};
use bytes::Bytes;
use futures_util::StreamExt;
use md5::{Digest, Md5};
use std::{
    io::{ErrorKind, SeekFrom},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tokio::{
    fs::{self, OpenOptions},
    io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
    task,
    time::timeout,
};
use tracing::debug;

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_path_for(output_path: &Path) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    output_path.with_file_name(format!("{name}.part-{}-{counter}", std::process::id()))
}

struct HashWorker {
    sender: Option<mpsc::Sender<Bytes>>,
    handle: task::JoinHandle<Box<str>>,
}

impl HashWorker {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel::<Bytes>();
        let handle = task::spawn_blocking(move || {
            let mut hasher = Md5::new();
            while let Ok(chunk) = receiver.recv() {
                hasher.update(&chunk);
            }
            hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
                .into_boxed_str()
        });
        Self {
            sender: Some(sender),
            handle,
        }
    }

    fn update(&self, data: Bytes) {
        if let Some(sender) = &self.sender {
            let _ = sender.send(data);
        }
    }

    async fn finalize(mut self) -> Result<Box<str>> {
        self.sender.take();
        self.handle.await.map_err(|err| {
            AppError::other_dynamic(format!("Hash worker failed: {err}").into_boxed_str())
        })
    }

    fn abort(mut self) {
        self.sender.take();
        self.handle.abort();
    }
}

fn abort_hash_worker(worker: &mut Option<HashWorker>) {
    if let Some(active) = worker.take() {
        active.abort();
    }
}

pub struct DownloadStreamResult {
    pub aborted: bool,
    pub hash: Option<Box<str>>,
    pub bytes_written: u64,
    pub temp_path: PathBuf,
}

/// Best-effort cleanup guard: ensures the temp file is removed from disk
/// when this guard is dropped while armed. Critical when a tokio task is
/// forcefully aborted mid-download, because no `await` past the abort point
/// runs — only Drop does.
struct TempFileGuard {
    path: PathBuf,
    armed: bool,
}

impl TempFileGuard {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempFileGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub async fn stream_download(
    response: reqwest::Response,
    output_path: &Path,
    content_length: Option<u64>,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    progress_timeout: Duration,
    shutdown: ShutdownToken,
) -> Result<DownloadStreamResult> {
    let temp_path = temp_path_for(output_path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .await?;
    let mut guard = TempFileGuard::new(temp_path.clone());
    let mut stream = response.bytes_stream();
    let mut downloaded: u64 = 0;
    let total = content_length.unwrap_or(0);
    let mut hash_worker = Some(HashWorker::new());

    let mut last_progress_bytes = 0u64;
    let mut last_progress_emitted = Instant::now();
    let mut last_progress_at = Instant::now();

    loop {
        if shutdown.is_cancelled() {
            abort_hash_worker(&mut hash_worker);
            file.shutdown().await?;
            let _ = fs::remove_file(&temp_path).await;
            return Ok(DownloadStreamResult {
                aborted: true,
                hash: None,
                bytes_written: downloaded,
                temp_path,
            });
        }

        let maybe_chunk = match timeout(progress_timeout, stream.next()).await {
            Ok(chunk) => chunk,
            Err(_) => {
                abort_hash_worker(&mut hash_worker);
                file.shutdown().await.ok();
                let _ = fs::remove_file(&temp_path).await;
                let stalled_for = last_progress_at.elapsed().as_secs();
                return Err(AppError::other_dynamic(
                    format!(
                        "Download stalled with no progress for {} seconds",
                        stalled_for.max(progress_timeout.as_secs())
                    )
                    .into_boxed_str(),
                ));
            }
        };

        let Some(chunk) = maybe_chunk else {
            break;
        };

        let chunk = match chunk {
            Ok(bytes) => bytes,
            Err(err) => {
                abort_hash_worker(&mut hash_worker);
                file.shutdown().await.ok();
                let _ = fs::remove_file(&temp_path).await;
                return Err(AppError::from(err));
            }
        };
        downloaded += chunk.len() as u64;

        if let Err(err) = file.write_all(&chunk).await {
            abort_hash_worker(&mut hash_worker);
            file.shutdown().await.ok();
            let _ = fs::remove_file(&temp_path).await;
            return Err(AppError::from(err));
        }

        if let Some(worker) = hash_worker.as_ref() {
            worker.update(chunk.clone());
        }

        if downloaded.saturating_sub(last_progress_bytes) >= MIN_PROGRESS_DELTA {
            last_progress_at = Instant::now();
        }

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

    if shutdown.is_cancelled() {
        abort_hash_worker(&mut hash_worker);
        file.shutdown().await.ok();
        let _ = fs::remove_file(&temp_path).await;
        return Ok(DownloadStreamResult {
            aborted: true,
            hash: None,
            bytes_written: downloaded,
            temp_path,
        });
    }

    if let Err(err) = file.flush().await {
        abort_hash_worker(&mut hash_worker);
        file.shutdown().await.ok();
        let _ = fs::remove_file(&temp_path).await;
        return Err(AppError::from(err));
    }

    if let Err(err) = file.sync_all().await {
        abort_hash_worker(&mut hash_worker);
        file.shutdown().await.ok();
        let _ = fs::remove_file(&temp_path).await;
        return Err(AppError::from(err));
    }

    if let Err(err) = file.shutdown().await {
        abort_hash_worker(&mut hash_worker);
        let _ = fs::remove_file(&temp_path).await;
        return Err(AppError::from(err));
    }

    if let Some(ref callback) = progress_callback
        && downloaded != last_progress_bytes
    {
        callback(downloaded, total);
    }

    let digest = match hash_worker.take() {
        Some(worker) => Some(worker.finalize().await?),
        None => None,
    };

    // success path — temp file will be renamed by the caller, so don't delete it.
    guard.disarm();

    Ok(DownloadStreamResult {
        aborted: false,
        hash: digest,
        bytes_written: downloaded,
        temp_path,
    })
}

pub async fn ensure_valid_archive(path: &Path, verify_zip_eocd: bool) -> Result<()> {
    let metadata = fs::metadata(path).await?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(AppError::other("Downloaded file is empty or invalid"));
    }

    let mut file = fs::File::open(path).await?;
    let mut header = [0u8; 64];
    let bytes_read = match file.read(&mut header).await {
        Ok(n) => n,
        Err(_) => {
            return Err(AppError::other("File too small to be a valid archive"));
        }
    };

    if bytes_read < 4 {
        return Err(AppError::other("File too small to be a valid archive"));
    }

    if header[..4] == [0x50, 0x4B, 0x03, 0x04] {
        if verify_zip_eocd {
            verify_zip_eocd_footer(&mut file, metadata.len()).await?;
        }
        return Ok(());
    }

    let header_slice = &header[..bytes_read];
    let trimmed = trim_leading_whitespace(header_slice);
    if trimmed.starts_with(b"<!DOCTYPE")
        || trimmed.starts_with(b"<!doctype")
        || trimmed.starts_with(b"<html")
        || trimmed.starts_with(b"<HTML")
    {
        return Err(AppError::other(
            "Received HTML error page instead of beatmap archive",
        ));
    }

    Err(AppError::other("Invalid archive: missing ZIP signature"))
}

async fn verify_zip_eocd_footer(file: &mut fs::File, file_size: u64) -> Result<()> {
    if file_size < 22 {
        return Err(AppError::other(
            "Invalid archive: missing central directory footer",
        ));
    }

    let search_len = MAX_EOCD_SEARCH_BYTES.min(file_size);
    file.seek(SeekFrom::End(-(search_len as i64))).await?;
    let mut buffer = vec![0u8; search_len as usize];
    file.read_exact(&mut buffer).await?;

    if buffer.windows(4).any(|window| window == EOCD_SIGNATURE) {
        Ok(())
    } else {
        Err(AppError::other(
            "Invalid archive: missing central directory footer",
        ))
    }
}

fn trim_leading_whitespace(data: &[u8]) -> &[u8] {
    let start = data
        .iter()
        .position(|&b| !matches!(b, b' ' | b'\t' | b'\n' | b'\r'))
        .unwrap_or(data.len());
    &data[start..]
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ArchiveValidationOptions {
    pub verify_zip_eocd: bool,
    pub remove_on_invalid: bool,
}

pub enum ArchiveValidationResult {
    Valid,
    NotFound,
    Invalid(String),
    Removed(String),
}

pub async fn validate_archive(
    path: &Path,
    options: ArchiveValidationOptions,
) -> Result<ArchiveValidationResult> {
    let metadata = match fs::metadata(path).await {
        Ok(meta) => meta,
        Err(err) if err.kind() == ErrorKind::NotFound => {
            return Ok(ArchiveValidationResult::NotFound);
        }
        Err(err) => return Err(AppError::from(err)),
    };

    if !metadata.is_file() {
        return handle_invalid(path, "Not a regular file", options.remove_on_invalid).await;
    }

    if metadata.len() == 0 {
        return handle_invalid(path, "File is empty", options.remove_on_invalid).await;
    }

    if let Err(err) = ensure_valid_archive(path, options.verify_zip_eocd).await {
        return handle_invalid(path, &err.to_string(), options.remove_on_invalid).await;
    }

    Ok(ArchiveValidationResult::Valid)
}

async fn handle_invalid(
    path: &Path,
    reason: &str,
    remove: bool,
) -> Result<ArchiveValidationResult> {
    if remove {
        match fs::remove_file(path).await {
            Ok(()) => {
                debug!(file = %path.display(), reason, "Removed invalid archive");
                return Ok(ArchiveValidationResult::Removed(reason.to_string()));
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                debug!(file = %path.display(), "Invalid file was already missing");
                return Ok(ArchiveValidationResult::Removed(reason.to_string()));
            }
            Err(err) => return Err(AppError::from(err)),
        }
    }
    Ok(ArchiveValidationResult::Invalid(reason.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_file_guard_removes_on_drop_when_armed() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "osu-collect-test-{}-{}.part",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, b"hello").unwrap();
        {
            let _guard = TempFileGuard::new(path.clone());
        }
        assert!(!path.exists(), "guard must remove file when dropped armed");
    }

    #[test]
    fn temp_file_guard_keeps_file_when_disarmed() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!(
            "osu-collect-test-{}-{}.part",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::write(&path, b"hello").unwrap();
        {
            let mut guard = TempFileGuard::new(path.clone());
            guard.disarm();
        }
        assert!(path.exists(), "disarmed guard must not remove the file");
        std::fs::remove_file(&path).unwrap();
    }
}
