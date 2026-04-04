//! Core download client logic

use crate::{
    mirrors::{Mirror, MirrorKind, MirrorPool},
    validation,
    worker::download_with_streaming,
    DownloadError, DownloadResult, Result, SkipReason,
};
use std::{path::Path, sync::Arc, time::Duration};
use tracing::{debug, warn};

/// Parameters for downloading a beatmapset
pub(crate) struct DownloadParams<'a> {
    pub beatmapset_id: u32,
    pub output_dir: &'a Path,
    pub client: &'a reqwest::Client,
    pub mirror_pool: &'a MirrorPool,
    pub verify_archive: bool,
    pub progress_timeout: Duration,
    pub progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    pub cancel_rx: tokio::sync::watch::Receiver<bool>,
}

/// Sanitize a filename from Content-Disposition header or generate default
fn sanitize_filename(raw: Option<&str>, beatmapset_id: u32) -> String {
    if let Some(name) = raw {
        // Basic sanitization
        name.chars()
            .map(|c| match c {
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                c => c,
            })
            .collect()
    } else {
        format!("{}.osz", beatmapset_id)
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

/// Download a single beatmapset with mirror fallback
pub async fn download_beatmapset(params: DownloadParams<'_>) -> Result<DownloadResult> {
    // Check if file already exists
    let potential_filename = format!("{}.osz", params.beatmapset_id);
    let potential_path = params.output_dir.join(&potential_filename);

    if potential_path.exists() {
        debug!(
            "Beatmapset {} already exists, skipping",
            params.beatmapset_id
        );
        return Ok(DownloadResult::Skipped {
            reason: SkipReason::AlreadyExists,
        });
    }

    // Get mirror plan
    let mirrors = params.mirror_pool.plan();
    if mirrors.is_empty() {
        return Err(DownloadError::AllMirrorsFailed {
            beatmapset_id: params.beatmapset_id,
        }
        .into());
    }

    let mut last_error = None;

    // Try each mirror
    for mirror in mirrors {
        // Check cancellation
        if *params.cancel_rx.borrow() {
            return Err(DownloadError::Cancelled.into());
        }

        debug!(
            "Attempting download of {} from {}",
            params.beatmapset_id,
            mirror.display_name()
        );

        match try_mirror(&mirror, &params).await {
            Ok(result) => return Ok(result),
            Err(err) => {
                warn!(
                    "Failed to download {} from {}: {}",
                    params.beatmapset_id,
                    mirror.display_name(),
                    err
                );

                // Check if rate limited
                if matches!(err, crate::Error::Download(DownloadError::RateLimited)) {
                    params.mirror_pool.mark_rate_limited(mirror.kind());
                }

                last_error = Some(err);
            }
        }
    }

    // All mirrors failed
    Err(last_error.unwrap_or_else(|| {
        DownloadError::AllMirrorsFailed {
            beatmapset_id: params.beatmapset_id,
        }
        .into()
    }))
}

/// Try downloading from a specific mirror
async fn try_mirror(mirror: &Mirror, params: &DownloadParams<'_>) -> Result<DownloadResult> {
    // Make HTTP request
    let url = mirror.url_for(params.beatmapset_id);
    let response = params.client.get(&url).send().await?;

    let status = response.status();

    // Handle status codes
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
        || (status == reqwest::StatusCode::FORBIDDEN
            && matches!(mirror.kind(), MirrorKind::Catboy(_)))
    {
        return Err(DownloadError::RateLimited.into());
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(DownloadResult::Skipped {
            reason: SkipReason::UnavailableOnMirrors,
        });
    }

    if !status.is_success() {
        return Err(DownloadError::http(format!("HTTP {}", status)).into());
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

    // Stream download
    let stream_result = download_with_streaming(
        response,
        &output_path,
        content_length,
        params.progress_callback.clone(),
        params.progress_timeout,
        params.cancel_rx.clone(),
    )
    .await?;

    if stream_result.cancelled {
        return Err(DownloadError::Cancelled.into());
    }

    // Verify archive if requested
    if params.verify_archive {
        validation::validate_zip_archive(&output_path).await?;
    }

    Ok(DownloadResult::Success {
        filename,
        size_bytes: stream_result.bytes_written,
        md5_hash: stream_result.hash,
    })
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
}
