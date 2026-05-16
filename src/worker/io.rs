use crate::{
    download::ShutdownToken,
    utils::{AppError, Result},
};
use std::{path::Path, sync::Arc, time::Duration};

pub use osu_downloader::{ArchiveValidationOptions, ArchiveValidationResult};

pub struct DownloadStreamResult {
    pub aborted: bool,
    pub hash: Option<Box<str>>,
    pub bytes_written: u64,
    pub temp_path: std::path::PathBuf,
}

pub async fn stream_download(
    response: reqwest::Response,
    output_path: &Path,
    content_length: Option<u64>,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    progress_timeout: Duration,
    shutdown: ShutdownToken,
) -> Result<DownloadStreamResult> {
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(shutdown.is_cancelled());
    if !shutdown.is_cancelled() {
        tokio::spawn(async move {
            shutdown.cancelled().await;
            let _ = cancel_tx.send(true);
        });
    }

    let result = osu_downloader::stream_download(
        response,
        output_path,
        content_length,
        progress_callback,
        progress_timeout,
        cancel_rx,
    )
    .await
    .map_err(to_app_error)?;

    Ok(DownloadStreamResult {
        aborted: result.aborted,
        hash: result.hash,
        bytes_written: result.bytes_written,
        temp_path: result.temp_path,
    })
}

pub async fn ensure_valid_archive(path: &Path, verify_zip_eocd: bool) -> Result<()> {
    osu_downloader::ensure_valid_archive(path, verify_zip_eocd)
        .await
        .map_err(to_app_error)
}

pub async fn validate_archive(
    path: &Path,
    options: ArchiveValidationOptions,
) -> Result<ArchiveValidationResult> {
    osu_downloader::validate_archive(path, options)
        .await
        .map_err(to_app_error)
}

fn to_app_error(err: osu_downloader::Error) -> AppError {
    AppError::other_dynamic(err.to_string().into_boxed_str())
}
