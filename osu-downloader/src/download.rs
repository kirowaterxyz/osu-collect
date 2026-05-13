//! Core download client logic

use crate::{
    mirrors::{Mirror, MirrorKind, MirrorPool},
    validation,
    worker::download_with_streaming,
    DownloadError, DownloadResult, Result, SkipReason,
};
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::time::sleep;
use tracing::{debug, warn};

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Parameters for downloading a beatmapset
pub(crate) struct DownloadParams<'a> {
    pub beatmapset_id: u32,
    pub output_dir: &'a Path,
    pub client: &'a reqwest::Client,
    pub mirror_pool: &'a MirrorPool,
    pub verify_archive: bool,
    pub progress_timeout: Duration,
    pub max_retries: u32,
    pub progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    pub cancel_rx: tokio::sync::watch::Receiver<bool>,
}

/// Sanitize a filename from Content-Disposition header or generate default
fn sanitize_filename(raw: Option<&str>, beatmapset_id: u32) -> String {
    let fallback = || format!("{beatmapset_id}.osz");

    let Some(name) = raw else {
        return fallback();
    };

    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c => c,
        })
        .collect();

    let is_safe = !sanitized.is_empty()
        && sanitized != "."
        && sanitized != ".."
        && !sanitized.starts_with('.')
        && std::path::Path::new(&sanitized).file_name() == Some(std::ffi::OsStr::new(&sanitized));

    if is_safe {
        sanitized
    } else {
        fallback()
    }
}

/// Extract filename from Content-Disposition header
fn extract_filename_from_header(header_value: &str) -> Option<String> {
    // Look for filename*=UTF-8''... or filename="..."
    for part in header_value.split(';') {
        let part = part.trim();

        if let Some(utf8_name) = part.strip_prefix("filename*=UTF-8''") {
            if let Ok(decoded) = urlencoding::decode(utf8_name) {
                return Some(decoded.into_owned());
            }
        }

        if let Some(quoted_name) = part.strip_prefix("filename=") {
            let name = quoted_name.trim_matches('"');
            return Some(name.to_string());
        }
    }
    None
}

fn matches_beatmapset_file_name(beatmapset_id: u32, name: &str) -> bool {
    let Some(rest) = name.strip_prefix(&beatmapset_id.to_string()) else {
        return false;
    };

    rest == ".osz" || rest.starts_with(' ') && rest.ends_with(".osz")
}

/// Download a single beatmapset with mirror fallback.
///
/// Returns the download result and the number of retry attempts made.
pub async fn download_beatmapset(params: DownloadParams<'_>) -> (Result<DownloadResult>, u32) {
    let (result, attempts) = download_beatmapset_inner(params).await;
    (result, attempts)
}

async fn download_beatmapset_inner(params: DownloadParams<'_>) -> (Result<DownloadResult>, u32) {
    match tokio::fs::read_dir(params.output_dir).await {
        Ok(mut dir) => loop {
            match dir.next_entry().await {
                Ok(Some(entry)) => {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    if matches_beatmapset_file_name(params.beatmapset_id, &name) {
                        debug!(
                            "beatmapset {} already exists ({}), skipping",
                            params.beatmapset_id, name
                        );
                        return (
                            Ok(DownloadResult::Skipped {
                                reason: SkipReason::AlreadyExists,
                            }),
                            0,
                        );
                    }
                }
                Ok(None) => break,
                Err(err) => return (Err(DownloadError::io(err.to_string()).into()), 0),
            }
        },
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return (Err(DownloadError::io(err.to_string()).into()), 0),
    }

    // Get mirror plan
    let mirrors = params.mirror_pool.plan();
    if mirrors.is_empty() {
        return (
            Err(DownloadError::AllMirrorsFailed {
                beatmapset_id: params.beatmapset_id,
            }
            .into()),
            0,
        );
    }

    let mut last_error = None;
    let mut attempted_mirrors = 0usize;
    let mut missed_mirrors = 0usize;
    let mut total_attempts: u32 = 0;

    // Try each mirror
    for mirror in mirrors {
        // Check cancellation
        if *params.cancel_rx.borrow() {
            return (Err(DownloadError::Cancelled.into()), total_attempts);
        }

        attempted_mirrors += 1;
        debug!(
            "Attempting download of {} from {}",
            params.beatmapset_id,
            mirror.display_name()
        );

        let mut attempt = 0u32;
        loop {
            match try_mirror(&mirror, &params).await {
                Ok(MirrorAttempt::Downloaded(result)) => return (Ok(result), total_attempts),
                Ok(MirrorAttempt::NotFound) => {
                    missed_mirrors += 1;
                    break;
                }
                Err(err) if should_retry(&err) && attempt < params.max_retries => {
                    // 429 on a retryable path: mark mirror and fall through to next mirror
                    if matches!(err, crate::Error::Download(DownloadError::RateLimited)) {
                        params.mirror_pool.mark_rate_limited(mirror.kind());
                        last_error = Some(err);
                        break;
                    }
                    attempt += 1;
                    total_attempts += 1;
                    warn!(
                        "Failed to download {} from {}: {}",
                        params.beatmapset_id,
                        mirror.display_name(),
                        err
                    );
                    let backoff = Duration::from_millis(500 * 2u64.saturating_pow(attempt))
                        .min(Duration::from_secs(8));
                    sleep(backoff).await;
                }
                Err(err) => {
                    warn!(
                        "Failed to download {} from {}: {}",
                        params.beatmapset_id,
                        mirror.display_name(),
                        err
                    );

                    if matches!(err, crate::Error::Download(DownloadError::RateLimited)) {
                        params.mirror_pool.mark_rate_limited(mirror.kind());
                    }

                    last_error = Some(err);
                    break;
                }
            }
        }
    }

    if last_error.is_none() && attempted_mirrors > 0 && missed_mirrors == attempted_mirrors {
        return (
            Ok(DownloadResult::Skipped {
                reason: SkipReason::UnavailableOnMirrors,
            }),
            total_attempts,
        );
    }

    let err = last_error.unwrap_or_else(|| {
        DownloadError::AllMirrorsFailed {
            beatmapset_id: params.beatmapset_id,
        }
        .into()
    });
    (Err(err), total_attempts)
}

#[derive(Debug)]
enum MirrorAttempt {
    Downloaded(DownloadResult),
    NotFound,
}

fn should_retry(err: &crate::Error) -> bool {
    match err {
        crate::Error::Http(err) => err.is_timeout() || err.is_connect(),
        crate::Error::Download(DownloadError::HttpStatus(s)) => {
            matches!(s, 429 | 500 | 502 | 503 | 504)
        }
        crate::Error::Download(DownloadError::ProgressTimeout) => true,
        _ => false,
    }
}

fn temp_path_for(output_path: &Path) -> PathBuf {
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let name = output_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("download");
    output_path.with_file_name(format!(
        "{name}.download-{}-{counter}.tmp",
        std::process::id()
    ))
}

/// Try downloading from a specific mirror
async fn try_mirror(mirror: &Mirror, params: &DownloadParams<'_>) -> Result<MirrorAttempt> {
    // Make HTTP request
    let url = mirror.url_for(params.beatmapset_id);
    let mut request = params.client.get(&url);
    if let Some(headers) = mirror.headers() {
        request = request.headers(headers.clone());
    }
    let response = request.send().await?;

    let status = response.status();

    // Handle status codes
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status == reqwest::StatusCode::FORBIDDEN
            && matches!(mirror.kind(), MirrorKind::Catboy(_)))
    {
        return Err(DownloadError::RateLimited.into());
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(MirrorAttempt::NotFound);
    }

    if !status.is_success() {
        return Err(DownloadError::HttpStatus(status.as_u16()).into());
    }

    // Reject HTML/JSON responses (captcha pages, maintenance notices)
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_ascii_lowercase());

    if let Some(ref ct) = content_type {
        let mime = ct.split(';').next().map(str::trim).unwrap_or("");
        if mime == "text/html" || mime == "application/json" {
            return Err(DownloadError::http(format!(
                "unexpected content type '{}' from {}",
                ct,
                mirror.display_name()
            ))
            .into());
        }
    }

    // Get content length and filename
    let content_length = response.content_length();

    let filename = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|h| h.to_str().ok())
        .and_then(extract_filename_from_header)
        .map(|f| sanitize_filename(Some(&f), params.beatmapset_id))
        .unwrap_or_else(|| sanitize_filename(None, params.beatmapset_id));

    let output_path = params.output_dir.join(&filename);

    let temp_path = temp_path_for(&output_path);
    let stream_result = match download_with_streaming(
        response,
        &temp_path,
        content_length,
        params.progress_callback.clone(),
        params.progress_timeout,
        params.cancel_rx.clone(),
    )
    .await
    {
        Ok(result) => result,
        Err(err) => {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(err);
        }
    };

    if stream_result.cancelled {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(DownloadError::Cancelled.into());
    }

    if let Some(expected) = content_length {
        if stream_result.bytes_written < expected {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(DownloadError::http(format!(
                "truncated response from {}: got {} of {} bytes",
                mirror.display_name(),
                stream_result.bytes_written,
                expected
            ))
            .into());
        }
    }

    if params.verify_archive {
        if let Err(err) = validation::validate_zip_archive(&temp_path).await {
            let _ = tokio::fs::remove_file(&temp_path).await;
            return Err(err);
        }
    }

    match finalize_download(&temp_path, &output_path).await {
        Ok(true) => {}
        Ok(false) => {
            return Ok(MirrorAttempt::Downloaded(DownloadResult::Skipped {
                reason: SkipReason::AlreadyExists,
            }));
        }
        Err(err) => return Err(err),
    }

    Ok(MirrorAttempt::Downloaded(DownloadResult::Success {
        filename,
        size_bytes: stream_result.bytes_written,
        md5_hash: stream_result.hash,
        mirror_used: mirror.kind(),
    }))
}

async fn finalize_download(temp_path: &Path, output_path: &Path) -> Result<bool> {
    let mut output = match tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output_path)
        .await
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Ok(false);
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(DownloadError::io(err.to_string()).into());
        }
    };

    let mut input = match tokio::fs::File::open(temp_path).await {
        Ok(input) => input,
        Err(err) => {
            let _ = tokio::fs::remove_file(output_path).await;
            let _ = tokio::fs::remove_file(temp_path).await;
            return Err(DownloadError::io(err.to_string()).into());
        }
    };

    if let Err(err) = tokio::io::copy(&mut input, &mut output).await {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return Err(DownloadError::io(err.to_string()).into());
    }

    if let Err(err) = output.sync_all().await {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return Err(DownloadError::io(err.to_string()).into());
    }

    if let Err(err) = tokio::fs::remove_file(temp_path).await {
        let _ = tokio::fs::remove_file(output_path).await;
        return Err(DownloadError::io(err.to_string()).into());
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename(None, 123), "123.osz");
        assert_eq!(
            sanitize_filename(Some("test/file.osz"), 456),
            "test_file.osz"
        );
        assert_eq!(sanitize_filename(Some(".."), 789), "789.osz");
        assert_eq!(sanitize_filename(Some("."), 789), "789.osz");
        assert_eq!(sanitize_filename(Some(""), 789), "789.osz");
        assert_eq!(sanitize_filename(Some("./map.osz"), 789), "789.osz");
        assert_eq!(sanitize_filename(Some("../etc/passwd"), 789), "789.osz");
        assert_eq!(
            sanitize_filename(Some("normal map.osz"), 789),
            "normal map.osz"
        );
        assert_eq!(
            sanitize_filename(Some("ユニコード.osz"), 789),
            "ユニコード.osz"
        );
    }

    #[test]
    fn test_extract_filename() {
        let header1 = "attachment; filename=\"test.osz\"";
        assert_eq!(
            extract_filename_from_header(header1),
            Some("test.osz".to_string())
        );

        let header2 = "attachment; filename*=UTF-8''test%20file.osz";
        assert_eq!(
            extract_filename_from_header(header2),
            Some("test file.osz".to_string())
        );
    }

    #[test]
    fn matches_exact_beatmapset_file_names() {
        assert!(matches_beatmapset_file_name(123, "123.osz"));
        assert!(matches_beatmapset_file_name(123, "123 artist.osz"));
        assert!(!matches_beatmapset_file_name(123, "1234.osz"));
        assert!(!matches_beatmapset_file_name(123, "123artist.osz"));
        assert!(!matches_beatmapset_file_name(123, "123 artist.zip"));
    }

    #[tokio::test]
    async fn finalize_download_preserves_existing_output() {
        let dir = std::env::temp_dir().join(format!(
            "osu-downloader-finalize-{}-{}",
            std::process::id(),
            TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        tokio::fs::create_dir(&dir).await.unwrap();

        let temp_path = dir.join("123.osz.tmp");
        let output_path = dir.join("123.osz");
        tokio::fs::write(&temp_path, b"new").await.unwrap();
        tokio::fs::write(&output_path, b"old").await.unwrap();

        let finalized = finalize_download(&temp_path, &output_path).await.unwrap();

        assert!(!finalized);
        assert_eq!(tokio::fs::read(&output_path).await.unwrap(), b"old");
        assert!(!tokio::fs::try_exists(&temp_path).await.unwrap());

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[test]
    fn partial_mirror_miss_prefers_last_error() {
        let last_error = Some(crate::Error::Download(DownloadError::HttpStatus(500)));
        let attempted_mirrors = 2;
        let missed_mirrors = 1;

        assert!(
            !(last_error.is_none() && attempted_mirrors > 0 && missed_mirrors == attempted_mirrors)
        );
    }

    #[test]
    fn all_mirror_misses_skip_without_error() {
        let last_error: Option<crate::Error> = None;
        let attempted_mirrors = 2;
        let missed_mirrors = 2;

        assert!(
            last_error.is_none() && attempted_mirrors > 0 && missed_mirrors == attempted_mirrors
        );
    }
}
