use crate::{
    check_shutdown,
    config::constants::{TRANSIENT_RETRY_ATTEMPTS, TRANSIENT_RETRY_BASE_DELAY},
    download::status,
    mirrors::{Mirror, MirrorKind, MirrorPool},
    utils::{
        AppError, FileExistsAction, Result, determine_file_exists_action, sanitize_filename_safe,
    },
    worker::{
        DownloadContext,
        io::{
            ArchiveValidationOptions, download_with_streaming, ensure_valid_archive,
            validate_archive,
        },
    },
};
use std::path::Path;
use std::{
    borrow::Cow,
    io::ErrorKind,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::{fs, time::sleep};
use tracing::trace;

pub type StatusCallback = Arc<dyn Fn(&str) + Send + Sync>;

fn emit_status(reporter: &Option<StatusCallback>, build: impl FnOnce() -> String) {
    if let Some(cb) = reporter {
        cb(&build());
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DownloadResult {
    Success(CompletedDownload),
    Skipped(Box<str>),
    Failed(DownloadFailure),
    /// All mirrors failed with transient network errors only — not a logical failure.
    NetworkError(String),
    Aborted,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadFailure {
    pub mirror: Option<MirrorKind>,
    pub reason: Cow<'static, str>,
}

impl DownloadResult {
    fn failed(mirror: Option<MirrorKind>, reason: impl Into<Cow<'static, str>>) -> Self {
        DownloadResult::Failed(DownloadFailure {
            mirror,
            reason: reason.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompletedDownload {
    pub filename: Box<str>,
    pub hash: Box<str>,
    pub mirror: MirrorKind,
    pub verify_duration_us: u64,
}

pub fn create_download_client() -> Result<reqwest::Client> {
    osu_downloader::http::create_download_client(None).map_err(|e| {
        AppError::other_dynamic(format!("failed to create download client: {e}").into_boxed_str())
    })
}

/// Per-mirror attempt outcome used inside `download_beatmap`.
enum MirrorAttempt {
    Done(DownloadResult),
    NotFound,
    RateLimited,
    Transient(String),
    Definitive(String),
}

pub async fn download_beatmap(
    beatmapset_id: u32,
    mirrors: &[Mirror],
    context: &DownloadContext,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    status_reporter: Option<StatusCallback>,
    rate_limiter: Option<MirrorPool>,
) -> Result<DownloadResult> {
    check_shutdown!(context.shutdown);

    let mut last_error: Option<DownloadResult> = None;
    let mut not_found_count: usize = 0;
    let total_mirrors = mirrors.len();
    let mut pending: Vec<Mirror> = mirrors.to_vec();
    let mut all_transient = true;
    let mut last_transient_reason = String::new();

    while !pending.is_empty() {
        let mut deferred_rate_limited: Vec<Mirror> = Vec::new();

        for mirror in pending.iter() {
            check_shutdown!(context.shutdown);

            match try_mirror_with_transient_retry(
                beatmapset_id,
                mirror,
                context,
                progress_callback.clone(),
                &status_reporter,
                rate_limiter.as_ref(),
            )
            .await?
            {
                MirrorAttempt::Done(result) => match result {
                    DownloadResult::Success(_) | DownloadResult::Skipped(_) => {
                        return Ok(result);
                    }
                    DownloadResult::Aborted => return Ok(DownloadResult::Aborted),
                    DownloadResult::Failed(_) | DownloadResult::NetworkError(_) => {
                        all_transient = false;
                        last_error = Some(result);
                    }
                },
                MirrorAttempt::NotFound => {
                    all_transient = false;
                    not_found_count += 1;
                    last_error = Some(DownloadResult::failed(
                        Some(mirror.kind()),
                        "Not found (404)",
                    ));
                }
                MirrorAttempt::RateLimited => {
                    all_transient = false;
                    deferred_rate_limited.push(mirror.clone());
                    last_error = Some(DownloadResult::failed(
                        Some(mirror.kind()),
                        status::RATE_LIMITED,
                    ));
                }
                MirrorAttempt::Transient(reason) => {
                    last_transient_reason = reason.clone();
                    last_error = Some(DownloadResult::failed(Some(mirror.kind()), reason));
                }
                MirrorAttempt::Definitive(reason) => {
                    all_transient = false;
                    last_error = Some(DownloadResult::failed(Some(mirror.kind()), reason));
                }
            }
        }

        if deferred_rate_limited.is_empty() {
            break;
        }

        let Some(ref limiter) = rate_limiter else {
            break;
        };

        let wait_duration = deferred_rate_limited
            .iter()
            .filter_map(|mirror| limiter.penalty_remaining(mirror.kind()))
            .min()
            .unwrap_or(Duration::from_secs(0));

        if !wait_duration.is_zero() {
            let deadline = tokio::time::Instant::now() + wait_duration;
            loop {
                if context.shutdown.is_cancelled() {
                    return Ok(DownloadResult::Aborted);
                }
                let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                if remaining.is_zero() {
                    break;
                }
                sleep(remaining.min(Duration::from_millis(100))).await;
            }
        }

        pending = deferred_rate_limited;
    }

    if not_found_count == total_mirrors && total_mirrors > 0 {
        return Ok(DownloadResult::failed(
            None,
            "Unavailable on all mirrors (404)",
        ));
    }

    if all_transient && !last_transient_reason.is_empty() {
        return Ok(DownloadResult::NetworkError(last_transient_reason));
    }

    Ok(last_error.unwrap_or_else(|| DownloadResult::failed(None, "All mirrors failed")))
}

async fn try_mirror_with_transient_retry(
    beatmapset_id: u32,
    mirror: &Mirror,
    context: &DownloadContext,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    status_reporter: &Option<StatusCallback>,
    rate_limiter: Option<&MirrorPool>,
) -> Result<MirrorAttempt> {
    let mut attempt: u8 = 0;

    loop {
        if context.shutdown.is_cancelled() {
            return Ok(MirrorAttempt::Done(DownloadResult::Aborted));
        }

        let outcome = try_mirror_once(
            beatmapset_id,
            mirror,
            context,
            progress_callback.clone(),
            status_reporter,
            rate_limiter,
        )
        .await?;

        match &outcome {
            MirrorAttempt::Transient(reason) if attempt + 1 < TRANSIENT_RETRY_ATTEMPTS => {
                attempt += 1;
                let backoff = TRANSIENT_RETRY_BASE_DELAY * (1u32 << (attempt - 1));
                trace!(
                    beatmapset_id,
                    mirror = mirror.display_name(),
                    attempt,
                    reason = %reason,
                    "retrying mirror after transient error"
                );
                emit_status(status_reporter, || {
                    format!(
                        "retrying {} after {} (attempt {}/{})",
                        mirror.display_name(),
                        reason,
                        attempt + 1,
                        TRANSIENT_RETRY_ATTEMPTS
                    )
                });
                if cancellable_sleep(backoff, context).await {
                    return Ok(MirrorAttempt::Done(DownloadResult::Aborted));
                }
                continue;
            }
            _ => return Ok(outcome),
        }
    }
}

async fn cancellable_sleep(duration: Duration, context: &DownloadContext) -> bool {
    let deadline = tokio::time::Instant::now() + duration;
    loop {
        if context.shutdown.is_cancelled() {
            return true;
        }
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return false;
        }
        sleep(remaining.min(Duration::from_millis(100))).await;
    }
}

async fn try_mirror_once(
    beatmapset_id: u32,
    mirror: &Mirror,
    context: &DownloadContext,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    status_reporter: &Option<StatusCallback>,
    rate_limiter: Option<&MirrorPool>,
) -> Result<MirrorAttempt> {
    emit_status(status_reporter, || {
        format!(
            "{} #{} from {}",
            status::FETCHING,
            beatmapset_id,
            mirror.display_name()
        )
    });

    let url = mirror.url_for(beatmapset_id);
    let mut req = context.client.get(&url);
    if let Some(extra_headers) = mirror.headers() {
        req = req.headers(extra_headers.clone());
    }
    let response = match req.send().await {
        Ok(resp) => resp,
        Err(e) => {
            if e.is_timeout() {
                return Ok(MirrorAttempt::Transient("Connection timeout".to_string()));
            }
            if e.is_connect() {
                return Ok(MirrorAttempt::Transient("Connection failed".to_string()));
            }
            return Ok(MirrorAttempt::Transient(format!(
                "Request failed on {}: {e}",
                mirror.display_name()
            )));
        }
    };

    let status = response.status();

    let catboy_rate_limited =
        matches!(mirror.kind(), MirrorKind::Catboy(_)) && status == reqwest::StatusCode::FORBIDDEN;

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || catboy_rate_limited {
        if let Some(limiter) = rate_limiter {
            limiter.mark_rate_limited(mirror.kind());
        }
        return Ok(MirrorAttempt::RateLimited);
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(MirrorAttempt::NotFound);
    }

    if status.is_server_error() {
        return Ok(MirrorAttempt::Transient(format!("HTTP {}", status)));
    }

    if !status.is_success() {
        return Ok(MirrorAttempt::Definitive(format!("HTTP {}", status)));
    }

    emit_status(status_reporter, || {
        format!(
            "{} #{} from {}",
            status::DOWNLOADING,
            beatmapset_id,
            mirror.display_name()
        )
    });

    let result =
        match process_mirror_response(mirror, response, beatmapset_id, context, progress_callback)
            .await
        {
            Ok(res) => res,
            Err(err) => {
                return Ok(MirrorAttempt::Definitive(format!(
                    "{} via {}",
                    err,
                    mirror.display_name()
                )));
            }
        };

    Ok(MirrorAttempt::Done(result))
}

async fn process_mirror_response(
    mirror: &Mirror,
    response: reqwest::Response,
    beatmapset_id: u32,
    context: &DownloadContext,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
) -> Result<DownloadResult> {
    let content_length = response.content_length();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase());

    if let Some(ref ct) = content_type
        && !is_archive_content_type(ct)
    {
        return Ok(DownloadResult::failed(
            Some(mirror.kind()),
            format!(
                "Unexpected content type '{}' from {}",
                ct,
                mirror.display_name()
            ),
        ));
    }

    let filename = extract_filename_from_response(&response, beatmapset_id)?;
    let sanitized_filename = sanitize_filename_safe(&filename, beatmapset_id);
    let output_path = context.output_dir.join(&sanitized_filename);

    let existing_metadata = match fs::metadata(&output_path).await {
        Ok(meta) => Some(meta),
        Err(err) if err.kind() == ErrorKind::NotFound => None,
        Err(err) => return Err(AppError::from(err)),
    };

    if existing_metadata.is_some() {
        check_shutdown!(context.shutdown);

        if context.skip_existing {
            if context.tracker.is_verified(beatmapset_id) {
                return Ok(DownloadResult::Skipped(
                    sanitized_filename.clone().into_boxed_str(),
                ));
            }
            let validation_opts = ArchiveValidationOptions {
                verify_zip_eocd: context.verify_zip_eocd,
                remove_on_invalid: true,
            };
            let _ = validate_archive(&output_path, validation_opts).await?;
        } else {
            let action = determine_file_exists_action(context.auto_overwrite);
            if matches!(action, FileExistsAction::Skip) {
                return Ok(DownloadResult::Skipped(
                    sanitized_filename.clone().into_boxed_str(),
                ));
            }
            fs::remove_file(&output_path).await?;
        }
    }

    write_and_verify_archive(
        mirror,
        response,
        context,
        output_path,
        sanitized_filename,
        content_length,
        progress_callback,
    )
    .await
}

async fn write_and_verify_archive(
    mirror: &Mirror,
    response: reqwest::Response,
    context: &DownloadContext,
    output_path: PathBuf,
    sanitized_filename: String,
    content_length: Option<u64>,
    progress_callback: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
) -> Result<DownloadResult> {
    context.cleanup_tracker.track(&output_path);

    let stream = match download_with_streaming(
        response,
        &output_path,
        content_length,
        progress_callback,
        context.progress_watchdog,
        context.shutdown.clone(),
    )
    .await
    {
        Ok(stream) => stream,
        Err(err) => {
            context.cleanup_tracker.forget(&output_path);
            return Err(err);
        }
    };

    context.cleanup_tracker.track(&stream.temp_path);

    if stream.aborted {
        context.cleanup_tracker.forget(&output_path);
        context.cleanup_tracker.forget(&stream.temp_path);
        return Ok(DownloadResult::Aborted);
    }

    if let Some(expected) = content_length
        && stream.bytes_written < expected
    {
        let _ = fs::remove_file(&stream.temp_path).await;
        context.cleanup_tracker.forget(&output_path);
        context.cleanup_tracker.forget(&stream.temp_path);
        return Ok(DownloadResult::failed(
            Some(mirror.kind()),
            format!(
                "download incomplete from {} (received {} of {} bytes)",
                mirror.display_name(),
                stream.bytes_written,
                expected
            ),
        ));
    }

    let verify_start = Instant::now();
    if let Err(err) = ensure_valid_archive(&stream.temp_path, context.verify_zip_eocd).await {
        let _ = fs::remove_file(&stream.temp_path).await;
        context.cleanup_tracker.forget(&output_path);
        context.cleanup_tracker.forget(&stream.temp_path);
        return Ok(DownloadResult::failed(
            Some(mirror.kind()),
            format!(
                "{} returned an invalid archive: {}",
                mirror.display_name(),
                err
            ),
        ));
    }
    let verify_duration_us = verify_start.elapsed().as_micros() as u64;

    let hash = stream.hash.unwrap_or_else(|| "unknown".into());

    if let Err(err) = fs::rename(&stream.temp_path, &output_path).await {
        let _ = fs::remove_file(&stream.temp_path).await;
        context.cleanup_tracker.forget(&output_path);
        context.cleanup_tracker.forget(&stream.temp_path);
        return Err(AppError::from(err));
    }

    context.cleanup_tracker.forget(&output_path);
    context.cleanup_tracker.forget(&stream.temp_path);

    Ok(DownloadResult::Success(CompletedDownload {
        filename: sanitized_filename.into_boxed_str(),
        hash,
        mirror: mirror.kind(),
        verify_duration_us,
    }))
}

fn is_archive_content_type(raw: &str) -> bool {
    let mime = raw.split(';').next().map(str::trim).unwrap_or("");

    matches!(
        mime,
        "application/x-osu-beatmap-archive"
            | "application/octet-stream"
            | "binary/octet-stream"
            | "application/zip"
            | "application/x-zip-compressed"
    )
}

fn extract_filename_from_response(
    response: &reqwest::Response,
    beatmapset_id: u32,
) -> Result<String> {
    let filename = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|v| v.to_str().ok())
        .and_then(|header| {
            header.split(';').find_map(|part| {
                let part = part.trim();
                part.strip_prefix("filename*=UTF-8''")
                    .or_else(|| part.strip_prefix("filename="))
                    .map(|s| s.trim_matches('"').to_string())
            })
        })
        .unwrap_or_else(|| format!("{}.osz", beatmapset_id));

    if Path::new(&filename)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("osz"))
    {
        Ok(filename)
    } else {
        Ok(format!("{}.osz", filename))
    }
}
