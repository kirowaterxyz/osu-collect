//! Per-beatmapset download state machine.
//!
//! Internal to the crate. `batch.rs` calls [`download_beatmapset`] for each item; results are
//! translated into the public [`DownloadEvent`](crate::DownloadEvent) stream there.

use crate::{
    config::{TRANSIENT_RETRY_ATTEMPTS, TRANSIENT_RETRY_BASE_DELAY},
    downloader::{BeatmapsetStatusEvent, FileExistsPolicy},
    mirrors::{Mirror, MirrorKind, MirrorPool},
    validation,
    worker::stream_download,
    SkipReason,
};
use std::{
    collections::HashSet,
    future::{pending, Future},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;
use tracing::{debug, trace};

const SIZE_PROBE_REDIRECT_LIMIT: usize = 4;

/// Internal callback bundle for a single beatmapset attempt.
#[derive(Clone, Default)]
pub(crate) struct BeatmapsetDownloadCallbacks {
    pub(crate) progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    pub(crate) status: Option<Arc<dyn Fn(BeatmapsetStatusEvent) + Send + Sync>>,
}

/// Internal per-attempt options.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BeatmapsetDownloadOptions {
    pub(crate) file_exists_policy: FileExistsPolicy,
}

impl Default for BeatmapsetDownloadOptions {
    fn default() -> Self {
        Self {
            file_exists_policy: FileExistsPolicy::Skip,
        }
    }
}

/// Internal outcome for a single beatmapset attempt.
#[derive(Debug, Clone)]
pub(crate) enum BeatmapsetDownloadOutcome {
    Success {
        filename: String,
        hash: String,
        mirror: MirrorKind,
        size_bytes: u64,
        verify_duration_us: u64,
    },
    Skipped {
        reason: SkipReason,
    },
    Failed {
        mirror: Option<MirrorKind>,
        reason: String,
    },
    NetworkError {
        reason: String,
    },
    Aborted,
}

pub(crate) struct DownloadParams<'a> {
    pub(crate) beatmapset_id: u32,
    pub(crate) output_dir: &'a Path,
    pub(crate) client: &'a reqwest::Client,
    pub(crate) mirror_pool: &'a MirrorPool,
    pub(crate) verify_archive: bool,
    pub(crate) verify_zip_eocd: bool,
    pub(crate) progress_timeout: Duration,
    pub(crate) callbacks: BeatmapsetDownloadCallbacks,
    pub(crate) options: BeatmapsetDownloadOptions,
    pub(crate) cancel_rx: tokio::sync::watch::Receiver<bool>,
}

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

/// Extract filename from Content-Disposition header.
///
/// Handles `filename*=UTF-8''...` and `filename="..."`.
pub fn extract_filename_from_header(header_value: &str) -> Option<String> {
    let mut filename = None;
    let mut extended_filename = None;

    for part in split_content_disposition(header_value) {
        let Some((name, value)) = part.split_once('=') else {
            continue;
        };

        let name = name.trim();
        let value = value.trim();

        if name.eq_ignore_ascii_case("filename*") {
            extended_filename = decode_extended_filename(value);
        } else if name.eq_ignore_ascii_case("filename") && filename.is_none() {
            filename = Some(decode_filename_value(value));
        }
    }

    extended_filename.or(filename)
}

fn split_content_disposition(header_value: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;

    for (index, ch) in header_value.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }

        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => {
                parts.push(header_value[start..index].trim());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }

    parts.push(header_value[start..].trim());
    parts
}

fn decode_extended_filename(value: &str) -> Option<String> {
    let (charset, rest) = value.split_once('\'')?;
    let (_, encoded) = rest.split_once('\'')?;

    if !charset.eq_ignore_ascii_case("utf-8") {
        return None;
    }

    urlencoding::decode(encoded)
        .ok()
        .map(|decoded| decoded.into_owned())
}

fn decode_filename_value(raw: &str) -> String {
    if raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2 {
        decode_quoted_string(&raw[1..raw.len() - 1])
    } else {
        raw.to_string()
    }
}

fn decode_quoted_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(escaped) = chars.next() {
                out.push(escaped);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn matches_beatmapset(beatmapset_id: u32, name: &str) -> bool {
    parse_beatmapset_id(name) == Some(beatmapset_id)
}

fn parse_beatmapset_id(name: &str) -> Option<u32> {
    let digits: String = name.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    let id: u32 = digits.parse().ok()?;
    let rest = &name[digits.len()..];
    if rest == ".osz" || (rest.starts_with(' ') && rest.ends_with(".osz")) {
        Some(id)
    } else {
        None
    }
}

pub(crate) async fn download_beatmapset(
    params: DownloadParams<'_>,
) -> (BeatmapsetDownloadOutcome, u32) {
    let mut cancel_rx = params.cancel_rx.clone();
    if *cancel_rx.borrow_and_update() {
        return (BeatmapsetDownloadOutcome::Aborted, 0);
    }

    let Some(create_dir_result) =
        run_cancelable(tokio::fs::create_dir_all(params.output_dir), &cancel_rx).await
    else {
        return (BeatmapsetDownloadOutcome::Aborted, 0);
    };
    if let Err(err) = create_dir_result {
        return (failed(None, err.to_string()), 0);
    }

    match find_existing_beatmapset(params.beatmapset_id, params.output_dir, &cancel_rx).await {
        ExistingCheck::Exists => match params.options.file_exists_policy {
            FileExistsPolicy::Skip => {
                return (
                    BeatmapsetDownloadOutcome::Skipped {
                        reason: SkipReason::AlreadyExists,
                    },
                    0,
                );
            }
            FileExistsPolicy::OverwriteTarget => {}
        },
        ExistingCheck::Missing => {}
        ExistingCheck::Aborted => return (BeatmapsetDownloadOutcome::Aborted, 0),
        ExistingCheck::Failed(reason) => return (failed(None, reason), 0),
    }

    let mut total_attempts = 0u32;
    let mut last_error: Option<BeatmapsetDownloadOutcome> = None;
    let mut not_found = HashSet::new();
    let mut all_transient = true;
    let mut last_transient_reason = String::new();

    let mut pending = params.mirror_pool.mirrors().to_vec();

    while !pending.is_empty() {
        if *cancel_rx.borrow_and_update() {
            return (BeatmapsetDownloadOutcome::Aborted, total_attempts);
        }

        let mut deferred_rate_limited = Vec::new();
        for mirror in &pending {
            if *cancel_rx.borrow_and_update() {
                return (BeatmapsetDownloadOutcome::Aborted, total_attempts);
            }

            match try_mirror_retry(mirror, &params, &mut total_attempts).await {
                MirrorAttempt::Done(outcome) => return (outcome, total_attempts),
                MirrorAttempt::NotFound => {
                    all_transient = false;
                    not_found.insert(mirror.kind());
                    let reason = "not found (404)".to_string();
                    emit_status(
                        &params.callbacks,
                        BeatmapsetStatusEvent::MirrorFailed {
                            mirror: mirror.kind(),
                            reason: reason.clone(),
                        },
                    );
                    last_error = Some(failed(Some(mirror.kind()), reason));
                }
                MirrorAttempt::RateLimited => {
                    all_transient = false;
                    deferred_rate_limited.push(mirror.clone());
                    params.mirror_pool.mark_rate_limited(mirror.kind());
                    let cooldown = mirror.kind().rate_limit_backoff();
                    emit_status(
                        &params.callbacks,
                        BeatmapsetStatusEvent::RateLimited {
                            mirror: mirror.kind(),
                            cooldown,
                        },
                    );
                    last_error = Some(failed(Some(mirror.kind()), "rate limited"));
                }
                MirrorAttempt::Transient(reason) => {
                    last_transient_reason = reason.clone();
                    emit_status(
                        &params.callbacks,
                        BeatmapsetStatusEvent::MirrorFailed {
                            mirror: mirror.kind(),
                            reason: reason.clone(),
                        },
                    );
                    last_error = Some(failed(Some(mirror.kind()), reason));
                }
                MirrorAttempt::Definitive(reason) => {
                    all_transient = false;
                    emit_status(
                        &params.callbacks,
                        BeatmapsetStatusEvent::MirrorFailed {
                            mirror: mirror.kind(),
                            reason: reason.clone(),
                        },
                    );
                    last_error = Some(failed(Some(mirror.kind()), reason));
                }
            }
        }

        if deferred_rate_limited.is_empty() {
            break;
        }

        let wait_duration = deferred_rate_limited
            .iter()
            .filter_map(|mirror| params.mirror_pool.penalty_remaining(mirror.kind()))
            .min()
            .unwrap_or(Duration::ZERO);

        if !wait_duration.is_zero() && sleep_cancelable(wait_duration, &cancel_rx).await {
            return (BeatmapsetDownloadOutcome::Aborted, total_attempts);
        }

        pending = deferred_rate_limited;
    }

    if not_found.len() == params.mirror_pool.mirrors_len() && params.mirror_pool.mirrors_len() > 0 {
        return (
            BeatmapsetDownloadOutcome::Skipped {
                reason: SkipReason::UnavailableOnMirrors,
            },
            total_attempts,
        );
    }

    if all_transient && !last_transient_reason.is_empty() {
        return (
            BeatmapsetDownloadOutcome::NetworkError {
                reason: last_transient_reason,
            },
            total_attempts,
        );
    }

    (
        last_error.unwrap_or_else(|| failed(None, "all mirrors failed")),
        total_attempts,
    )
}

#[derive(Debug)]
enum MirrorAttempt {
    Done(BeatmapsetDownloadOutcome),
    NotFound,
    RateLimited,
    Transient(String),
    Definitive(String),
}

async fn find_existing_beatmapset(
    beatmapset_id: u32,
    output_dir: &Path,
    cancel_rx: &tokio::sync::watch::Receiver<bool>,
) -> ExistingCheck {
    let Some(read_dir_result) = run_cancelable(tokio::fs::read_dir(output_dir), cancel_rx).await
    else {
        return ExistingCheck::Aborted;
    };

    let mut dir = match read_dir_result {
        Ok(dir) => dir,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return ExistingCheck::Missing,
        Err(err) => return ExistingCheck::Failed(err.to_string()),
    };

    loop {
        let Some(entry_result) = run_cancelable(dir.next_entry(), cancel_rx).await else {
            return ExistingCheck::Aborted;
        };
        let entry = match entry_result {
            Ok(Some(entry)) => entry,
            Ok(None) => return ExistingCheck::Missing,
            Err(err) => return ExistingCheck::Failed(err.to_string()),
        };
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches_beatmapset(beatmapset_id, &name) {
            debug!(beatmapset_id, file = %name, "beatmapset already exists");
            return ExistingCheck::Exists;
        }
    }
}

enum ExistingCheck {
    Exists,
    Missing,
    Aborted,
    Failed(String),
}

async fn try_mirror_retry(
    mirror: &Mirror,
    params: &DownloadParams<'_>,
    total_attempts: &mut u32,
) -> MirrorAttempt {
    let mut retry = 0u32;

    loop {
        if *params.cancel_rx.borrow() {
            return MirrorAttempt::Done(BeatmapsetDownloadOutcome::Aborted);
        }

        *total_attempts += 1;
        let outcome = try_mirror_once(mirror, params).await;

        match outcome {
            MirrorAttempt::Transient(reason) if retry + 1 < TRANSIENT_RETRY_ATTEMPTS => {
                retry += 1;
                let backoff = TRANSIENT_RETRY_BASE_DELAY * (1u32 << (retry - 1));
                trace!(
                    beatmapset_id = params.beatmapset_id,
                    mirror = mirror.display_name(),
                    retry,
                    reason = %reason,
                    "retrying mirror after transient error"
                );
                emit_status(
                    &params.callbacks,
                    BeatmapsetStatusEvent::RetryingTransient {
                        mirror: mirror.kind(),
                        attempt: retry + 1,
                        max_attempts: TRANSIENT_RETRY_ATTEMPTS,
                        reason,
                    },
                );
                if sleep_cancelable(backoff, &params.cancel_rx).await {
                    return MirrorAttempt::Done(BeatmapsetDownloadOutcome::Aborted);
                }
            }
            other => return other,
        }
    }
}

async fn try_mirror_once(mirror: &Mirror, params: &DownloadParams<'_>) -> MirrorAttempt {
    emit_status(
        &params.callbacks,
        BeatmapsetStatusEvent::Contacting {
            mirror: mirror.kind(),
        },
    );

    let url = mirror.url_for(params.beatmapset_id);
    let mut request = params.client.get(&url);
    if let Some(headers) = mirror.headers() {
        request = request.headers(headers.clone());
    }

    let Some(response) = run_cancelable(request.send(), &params.cancel_rx).await else {
        return MirrorAttempt::Done(BeatmapsetDownloadOutcome::Aborted);
    };
    let response = match response {
        Ok(response) => response,
        Err(err) if err.is_timeout() => {
            return MirrorAttempt::Transient("connection timeout".to_string())
        }
        Err(err) if err.is_connect() => {
            return MirrorAttempt::Transient("connection failed".to_string())
        }
        Err(err) => {
            return MirrorAttempt::Transient(format!(
                "request failed on {}: {err}",
                mirror.display_name()
            ))
        }
    };
    let probed_size = if response.content_length().is_none() {
        probe_download_size(mirror, params).await
    } else {
        None
    };

    let status = response.status();
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return MirrorAttempt::RateLimited;
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        return MirrorAttempt::NotFound;
    }

    if status.is_server_error() {
        return MirrorAttempt::Transient(format!("HTTP {status}"));
    }

    if !status.is_success() {
        return MirrorAttempt::Definitive(format!("HTTP {status}"));
    }

    match process_mirror_response(mirror, response, params, probed_size).await {
        Ok(outcome) => MirrorAttempt::Done(outcome),
        Err(reason) => MirrorAttempt::Definitive(reason),
    }
}

async fn probe_download_size(mirror: &Mirror, params: &DownloadParams<'_>) -> Option<u64> {
    let mut url = mirror.url_for(params.beatmapset_id);

    for _ in 0..=SIZE_PROBE_REDIRECT_LIMIT {
        let mut request = params
            .client
            .get(&url)
            .header(reqwest::header::RANGE, "bytes=0-0")
            .header(reqwest::header::ACCEPT_ENCODING, "identity")
            .header(reqwest::header::CONNECTION, "close");
        if let Some(headers) = mirror.headers() {
            request = request.headers(headers.clone());
        }

        let Some(result) = run_cancelable(request.send(), &params.cancel_rx).await else {
            return None;
        };
        let response = match result {
            Ok(response) => response,
            Err(err) => {
                trace!(beatmapset_id = params.beatmapset_id, mirror = %mirror.display_name(), error = %err, "failed to probe download size");
                return None;
            }
        };

        if response.status().is_redirection() {
            let Some(location) = response
                .headers()
                .get(reqwest::header::LOCATION)
                .and_then(|value| value.to_str().ok())
                .and_then(|location| response.url().join(location).ok())
            else {
                return None;
            };
            url = location.to_string();
            continue;
        }

        return size_from_headers(response.headers());
    }

    None
}

fn size_from_headers(headers: &reqwest::header::HeaderMap) -> Option<u64> {
    headers
        .get(reqwest::header::CONTENT_RANGE)
        .and_then(|value| value.to_str().ok())
        .and_then(size_from_content_range)
        .or_else(|| {
            headers
                .get(reqwest::header::CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.parse().ok())
        })
}

fn size_from_content_range(value: &str) -> Option<u64> {
    value.rsplit_once('/')?.1.parse().ok()
}

async fn process_mirror_response(
    mirror: &Mirror,
    response: reqwest::Response,
    params: &DownloadParams<'_>,
    probed_size: Option<u64>,
) -> std::result::Result<BeatmapsetDownloadOutcome, String> {
    let content_length = response.content_length().or(probed_size);
    if let Some(content_type) = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.to_ascii_lowercase())
    {
        if !is_archive_content_type(&content_type) {
            return Err(format!(
                "unexpected content type '{content_type}' from {}",
                mirror.display_name()
            ));
        }
    }

    let filename = extract_filename(&response, params.beatmapset_id);
    let sanitized_filename = sanitize_filename(Some(&filename), params.beatmapset_id);
    let output_path = params.output_dir.join(&sanitized_filename);

    if let Some(metadata_result) =
        run_cancelable(tokio::fs::metadata(&output_path), &params.cancel_rx).await
    {
        match metadata_result {
            Ok(_) => match params.options.file_exists_policy {
                FileExistsPolicy::Skip => {
                    return Ok(BeatmapsetDownloadOutcome::Skipped {
                        reason: SkipReason::AlreadyExists,
                    });
                }
                FileExistsPolicy::OverwriteTarget => {
                    if let Some(remove_result) =
                        run_cancelable(tokio::fs::remove_file(&output_path), &params.cancel_rx)
                            .await
                    {
                        remove_result.map_err(|err| err.to_string())?;
                    } else {
                        return Ok(BeatmapsetDownloadOutcome::Aborted);
                    }
                }
            },
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(err) => return Err(err.to_string()),
        }
    } else {
        return Ok(BeatmapsetDownloadOutcome::Aborted);
    }

    emit_status(
        &params.callbacks,
        BeatmapsetStatusEvent::Downloading {
            mirror: mirror.kind(),
        },
    );

    write_archive(
        mirror,
        response,
        params,
        output_path,
        sanitized_filename,
        content_length,
    )
    .await
}

async fn write_archive(
    mirror: &Mirror,
    response: reqwest::Response,
    params: &DownloadParams<'_>,
    output_path: PathBuf,
    filename: String,
    content_length: Option<u64>,
) -> std::result::Result<BeatmapsetDownloadOutcome, String> {
    let stream = stream_download(
        response,
        &output_path,
        content_length,
        params.callbacks.progress.clone(),
        params.progress_timeout,
        params.cancel_rx.clone(),
    )
    .await
    .map_err(|err| err.to_string())?;

    if stream.aborted {
        return Ok(BeatmapsetDownloadOutcome::Aborted);
    }

    if let Some(expected) = content_length {
        if stream.bytes_written < expected {
            let _ = tokio::fs::remove_file(&stream.temp_path).await;
            return Err(format!(
                "download incomplete from {} (received {} of {} bytes)",
                mirror.display_name(),
                stream.bytes_written,
                expected
            ));
        }
    }

    emit_status(
        &params.callbacks,
        BeatmapsetStatusEvent::Verifying {
            mirror: mirror.kind(),
        },
    );

    let verify_start = Instant::now();
    if params.verify_archive {
        if let Some(validate_result) = run_cancelable(
            validation::ensure_valid_archive(&stream.temp_path, params.verify_zip_eocd),
            &params.cancel_rx,
        )
        .await
        {
            if let Err(err) = validate_result {
                let _ = tokio::fs::remove_file(&stream.temp_path).await;
                return Err(format!(
                    "{} returned an invalid archive: {err}",
                    mirror.display_name()
                ));
            }
        } else {
            let _ = tokio::fs::remove_file(&stream.temp_path).await;
            return Ok(BeatmapsetDownloadOutcome::Aborted);
        }
    }
    let verify_duration_us = verify_start.elapsed().as_micros() as u64;

    match finalize_download(&stream.temp_path, &output_path, &params.cancel_rx).await {
        FinalizeResult::Done => {}
        FinalizeResult::AlreadyExists => {
            return Ok(BeatmapsetDownloadOutcome::Skipped {
                reason: SkipReason::AlreadyExists,
            });
        }
        FinalizeResult::Aborted => return Ok(BeatmapsetDownloadOutcome::Aborted),
        FinalizeResult::Failed(reason) => return Err(reason),
    }

    Ok(BeatmapsetDownloadOutcome::Success {
        filename,
        hash: stream
            .hash
            .unwrap_or_else(|| "unknown".into())
            .into_string(),
        mirror: mirror.kind(),
        size_bytes: stream.bytes_written,
        verify_duration_us,
    })
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

fn extract_filename(response: &reqwest::Response, beatmapset_id: u32) -> String {
    let filename = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(extract_filename_from_header)
        .unwrap_or_else(|| format!("{beatmapset_id}.osz"));

    if filename
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("osz"))
    {
        filename
    } else {
        format!("{filename}.osz")
    }
}

async fn finalize_download(
    temp_path: &Path,
    output_path: &Path,
    cancel_rx: &tokio::sync::watch::Receiver<bool>,
) -> FinalizeResult {
    let Some(link_result) =
        run_cancelable(tokio::fs::hard_link(temp_path, output_path), cancel_rx).await
    else {
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Aborted;
    };

    match link_result {
        Ok(()) => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return FinalizeResult::Done;
        }
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return FinalizeResult::AlreadyExists;
        }
        Err(err)
            if matches!(
                err.kind(),
                std::io::ErrorKind::CrossesDevices | std::io::ErrorKind::Unsupported
            ) => {}
        Err(err) => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return FinalizeResult::Failed(err.to_string());
        }
    }

    copy_download(temp_path, output_path, cancel_rx).await
}

async fn copy_download(
    temp_path: &Path,
    output_path: &Path,
    cancel_rx: &tokio::sync::watch::Receiver<bool>,
) -> FinalizeResult {
    let Some(output_result) = run_cancelable(
        tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output_path),
        cancel_rx,
    )
    .await
    else {
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Aborted;
    };

    let mut output = match output_result {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return FinalizeResult::AlreadyExists;
        }
        Err(err) => {
            let _ = tokio::fs::remove_file(temp_path).await;
            return FinalizeResult::Failed(err.to_string());
        }
    };

    let Some(input_result) = run_cancelable(tokio::fs::File::open(temp_path), cancel_rx).await
    else {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Aborted;
    };
    let mut input = match input_result {
        Ok(input) => input,
        Err(err) => {
            let _ = tokio::fs::remove_file(output_path).await;
            let _ = tokio::fs::remove_file(temp_path).await;
            return FinalizeResult::Failed(err.to_string());
        }
    };

    let Some(copy_result) =
        run_cancelable(tokio::io::copy(&mut input, &mut output), cancel_rx).await
    else {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Aborted;
    };
    if let Err(err) = copy_result {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Failed(err.to_string());
    }

    let Some(sync_result) = run_cancelable(output.sync_all(), cancel_rx).await else {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Aborted;
    };
    if let Err(err) = sync_result {
        let _ = tokio::fs::remove_file(output_path).await;
        let _ = tokio::fs::remove_file(temp_path).await;
        return FinalizeResult::Failed(err.to_string());
    }

    let _ = tokio::fs::remove_file(temp_path).await;
    FinalizeResult::Done
}

enum FinalizeResult {
    Done,
    AlreadyExists,
    Aborted,
    Failed(String),
}

fn failed(mirror: Option<MirrorKind>, reason: impl Into<String>) -> BeatmapsetDownloadOutcome {
    BeatmapsetDownloadOutcome::Failed {
        mirror,
        reason: reason.into(),
    }
}

fn emit_status(callbacks: &BeatmapsetDownloadCallbacks, event: BeatmapsetStatusEvent) {
    if let Some(callback) = &callbacks.status {
        callback(event);
    }
}

async fn sleep_cancelable(
    duration: Duration,
    cancel_rx: &tokio::sync::watch::Receiver<bool>,
) -> bool {
    run_cancelable(sleep(duration), cancel_rx).await.is_none()
}

async fn run_cancelable<T>(
    future: impl Future<Output = T>,
    cancel_rx: &tokio::sync::watch::Receiver<bool>,
) -> Option<T> {
    let mut cancel_rx = cancel_rx.clone();
    tokio::select! {
        biased;
        _ = wait_until_cancelled(&mut cancel_rx) => None,
        result = future => Some(result),
    }
}

async fn wait_until_cancelled(cancel_rx: &mut tokio::sync::watch::Receiver<bool>) {
    loop {
        if *cancel_rx.borrow_and_update() {
            return;
        }
        if cancel_rx.changed().await.is_err() {
            pending::<()>().await;
        }
    }
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
        assert_eq!(
            extract_filename_from_header("attachment; filename=\"test.osz\""),
            Some("test.osz".to_string())
        );

        assert_eq!(
            extract_filename_from_header("attachment; filename*=UTF-8''test%20file.osz"),
            Some("test file.osz".to_string())
        );

        assert_eq!(
            extract_filename_from_header(r#"attachment; filename="foo\"bar.osz""#),
            Some(r#"foo"bar.osz"#.to_string())
        );

        assert_eq!(
            extract_filename_from_header(r#"attachment; filename="foo\\bar.osz""#),
            Some(r#"foo\bar.osz"#.to_string())
        );

        assert_eq!(
            extract_filename_from_header("attachment; filename=plain.osz"),
            Some("plain.osz".to_string())
        );

        assert_eq!(
            extract_filename_from_header(r#"attachment; filename="artist - title; diff.osz""#),
            Some("artist - title; diff.osz".to_string())
        );

        assert_eq!(
            extract_filename_from_header(
                "attachment; filename=plain.osz; filename*=utf-8''encoded%20name.osz"
            ),
            Some("encoded name.osz".to_string())
        );

        assert_eq!(
            extract_filename_from_header(
                "attachment; filename=fallback.osz; filename*=iso-8859-1''ignored.osz"
            ),
            Some("fallback.osz".to_string())
        );

        assert_eq!(
            extract_filename_from_header("attachment; FILENAME=upper.osz"),
            Some("upper.osz".to_string())
        );
    }

    #[test]
    fn matches_exact_beatmapset_file_names() {
        assert!(matches_beatmapset(123, "123.osz"));
        assert!(matches_beatmapset(123, "123 artist.osz"));
        assert!(!matches_beatmapset(123, "1234.osz"));
        assert!(!matches_beatmapset(123, "123artist.osz"));
        assert!(!matches_beatmapset(123, "123 artist.zip"));
    }

    #[test]
    fn archive_content_type_accepts_known_archive_mimes() {
        assert!(is_archive_content_type("application/x-osu-beatmap-archive"));
        assert!(is_archive_content_type(
            "application/octet-stream; charset=binary"
        ));
        assert!(is_archive_content_type("binary/octet-stream"));
        assert!(is_archive_content_type("application/zip"));
        assert!(is_archive_content_type("application/x-zip-compressed"));
        assert!(!is_archive_content_type("text/html"));
        assert!(!is_archive_content_type("application/json"));
    }

    #[test]
    fn size_from_content_range_uses_complete_length() {
        assert_eq!(
            size_from_content_range("bytes 0-0/24413678"),
            Some(24_413_678)
        );
        assert_eq!(size_from_content_range("bytes 0-3/*"), None);
        assert_eq!(size_from_content_range("invalid"), None);
    }

    #[tokio::test]
    async fn range_probe_discovers_redirected_download_size() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 1024];
                let n = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..n]);
                if request.starts_with("GET /mirror/") {
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 302 Found\r\nLocation: http://{addr}/archive/42\r\nContent-Length: 0\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .unwrap();
                } else if request.starts_with("GET /archive/") {
                    stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/10000000\r\nContent-Length: 1\r\n\r\nP").unwrap();
                }
            }
        });

        let client = reqwest::Client::new();
        let mirror = Mirror::custom(format!("http://{addr}/mirror/{{id}}")).unwrap();
        let mirror_pool = MirrorPool::new(vec![mirror.clone()]);
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let dir = tempfile::tempdir().unwrap();
        let params = DownloadParams {
            beatmapset_id: 42,
            output_dir: dir.path(),
            client: &client,
            mirror_pool: &mirror_pool,
            verify_archive: false,
            verify_zip_eocd: false,
            progress_timeout: Duration::from_secs(1),
            callbacks: BeatmapsetDownloadCallbacks::default(),
            options: BeatmapsetDownloadOptions::default(),
            cancel_rx,
        };

        assert_eq!(
            probe_download_size(&mirror, &params).await,
            Some(10_000_000)
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn probe_preserves_range_across_multiple_redirects() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 2048];
                let n = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..n]);
                if request.starts_with("GET /api/") {
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 302 Found\r\nLocation: http://{addr}/dl/997762\r\nContent-Length: 0\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .unwrap();
                } else if request.starts_with("GET /dl/") {
                    stream
                        .write_all(
                            format!(
                                "HTTP/1.1 302 Found\r\nLocation: http://{addr}/s3/997762.osz\r\nContent-Length: 0\r\n\r\n"
                            )
                            .as_bytes(),
                        )
                        .unwrap();
                } else if request.starts_with("GET /s3/") {
                    assert!(request.contains("Range: bytes=0-0"));
                    stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/44911016\r\nContent-Length: 1\r\n\r\nP").unwrap();
                }
            }
        });

        let client = reqwest::Client::new();
        let mirror = Mirror::custom(format!("http://{addr}/api/{{id}}")).unwrap();
        let mirror_pool = MirrorPool::new(vec![mirror.clone()]);
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let dir = tempfile::tempdir().unwrap();
        let params = DownloadParams {
            beatmapset_id: 997762,
            output_dir: dir.path(),
            client: &client,
            mirror_pool: &mirror_pool,
            verify_archive: false,
            verify_zip_eocd: false,
            progress_timeout: Duration::from_secs(1),
            callbacks: BeatmapsetDownloadCallbacks::default(),
            options: BeatmapsetDownloadOptions::default(),
            cancel_rx,
        };

        assert_eq!(
            probe_download_size(&mirror, &params).await,
            Some(44_911_016)
        );
        server.join().unwrap();
    }

    #[tokio::test]
    async fn completion_uses_probed_size_when_download_is_chunked() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{Arc, Mutex},
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 2048];
                let n = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..n]);
                if request.contains("Range: bytes=0-0") {
                    stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/262144\r\nContent-Length: 1\r\n\r\nP").unwrap();
                } else {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=42.osz\r\nTransfer-Encoding: chunked\r\n\r\n40000\r\n").unwrap();
                    stream.write_all(&vec![b'a'; 262_144]).unwrap();
                    let _ = stream.write_all(b"\r\n0\r\n\r\n");
                }
            }
        });

        let client = reqwest::Client::new();
        let mirror = Mirror::custom(format!("http://{addr}/download/{{id}}")).unwrap();
        let mirror_pool = MirrorPool::new(vec![mirror]);
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let progress = Arc::new(Mutex::new(Vec::new()));
        let progress_events = progress.clone();
        let dir = tempfile::tempdir().unwrap();

        let (outcome, _) = download_beatmapset(DownloadParams {
            beatmapset_id: 42,
            output_dir: dir.path(),
            client: &client,
            mirror_pool: &mirror_pool,
            verify_archive: false,
            verify_zip_eocd: false,
            progress_timeout: Duration::from_secs(1),
            callbacks: BeatmapsetDownloadCallbacks {
                progress: Some(Arc::new(move |downloaded, total| {
                    progress_events.lock().unwrap().push((downloaded, total));
                })),
                status: None,
            },
            options: BeatmapsetDownloadOptions::default(),
            cancel_rx,
        })
        .await;

        assert!(matches!(
            outcome,
            BeatmapsetDownloadOutcome::Success {
                size_bytes: 262_144,
                ..
            }
        ));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn skip_existing_file_does_not_emit_downloading() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{Arc, Mutex},
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=custom.osz\r\nContent-Length: 0\r\n\r\n",
                )
                .unwrap();
        });

        let client = reqwest::Client::new();
        let mirror = Mirror::custom(format!("http://{addr}/download/{{id}}")).unwrap();
        let mirror_pool = MirrorPool::new(vec![mirror]);
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let statuses = Arc::new(Mutex::new(Vec::new()));
        let status_events = statuses.clone();
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("custom.osz"), b"existing").unwrap();

        let (outcome, _) = download_beatmapset(DownloadParams {
            beatmapset_id: 42,
            output_dir: dir.path(),
            client: &client,
            mirror_pool: &mirror_pool,
            verify_archive: false,
            verify_zip_eocd: false,
            progress_timeout: Duration::from_secs(1),
            callbacks: BeatmapsetDownloadCallbacks {
                progress: None,
                status: Some(Arc::new(move |status| {
                    status_events.lock().unwrap().push(status);
                })),
            },
            options: BeatmapsetDownloadOptions {
                file_exists_policy: FileExistsPolicy::Skip,
            },
            cancel_rx,
        })
        .await;

        assert!(matches!(
            outcome,
            BeatmapsetDownloadOutcome::Skipped {
                reason: SkipReason::AlreadyExists
            }
        ));
        assert!(!statuses
            .lock()
            .unwrap()
            .iter()
            .any(|status| matches!(status, BeatmapsetStatusEvent::Downloading { .. })));
        server.join().unwrap();
    }

    #[tokio::test]
    async fn finalize_download_preserves_existing_output() {
        let dir = std::env::temp_dir().join(format!(
            "osu-downloader-finalize-{}-{:?}",
            std::process::id(),
            std::time::SystemTime::now()
        ));
        tokio::fs::create_dir(&dir).await.unwrap();

        let temp_path = dir.join("123.osz.tmp");
        let output_path = dir.join("123.osz");
        tokio::fs::write(&temp_path, b"new").await.unwrap();
        tokio::fs::write(&output_path, b"old").await.unwrap();
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let finalized = finalize_download(&temp_path, &output_path, &cancel_rx).await;

        assert!(matches!(finalized, FinalizeResult::AlreadyExists));
        assert_eq!(tokio::fs::read(&output_path).await.unwrap(), b"old");
        assert!(!tokio::fs::try_exists(&temp_path).await.unwrap());

        tokio::fs::remove_dir_all(&dir).await.unwrap();
    }

    #[tokio::test]
    async fn rate_limited_mirror_is_retried_after_other_mirrors_fail() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
            thread,
        };

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let rate_hits = Arc::new(AtomicUsize::new(0));
        let missing_hits = Arc::new(AtomicUsize::new(0));
        let server_rate_hits = rate_hits.clone();
        let server_missing_hits = missing_hits.clone();
        let server = thread::spawn(move || {
            for _ in 0..3 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut request = [0u8; 1024];
                let n = stream.read(&mut request).unwrap();
                let request = String::from_utf8_lossy(&request[..n]);
                if request.starts_with("GET /rate/") {
                    let hit = server_rate_hits.fetch_add(1, Ordering::SeqCst);
                    if hit == 0 {
                        stream
                            .write_all(
                                b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n",
                            )
                            .unwrap();
                    } else {
                        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=123.osz\r\nContent-Length: 4\r\n\r\ndata").unwrap();
                    }
                } else if request.starts_with("GET /missing/") {
                    server_missing_hits.fetch_add(1, Ordering::SeqCst);
                    stream
                        .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                        .unwrap();
                } else {
                    stream
                        .write_all(
                            b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n",
                        )
                        .unwrap();
                }
            }
        });

        let rate_limited_then_ok = Mirror {
            kind: MirrorKind::Nerinyan,
            template: format!("http://{addr}/rate/{{id}}").into_boxed_str(),
            headers: None,
        };
        let missing = Mirror {
            kind: MirrorKind::OsuDirect,
            template: format!("http://{addr}/missing/{{id}}").into_boxed_str(),
            headers: None,
        };
        let mirror_pool = MirrorPool::new(vec![rate_limited_then_ok, missing]);
        let dir = tempfile::tempdir().unwrap();
        let client = reqwest::Client::new();
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        let (outcome, _) = download_beatmapset(DownloadParams {
            beatmapset_id: 123,
            output_dir: dir.path(),
            client: &client,
            mirror_pool: &mirror_pool,
            verify_archive: false,
            verify_zip_eocd: false,
            progress_timeout: Duration::from_secs(1),
            callbacks: BeatmapsetDownloadCallbacks::default(),
            options: BeatmapsetDownloadOptions::default(),
            cancel_rx,
        })
        .await;

        assert!(matches!(outcome, BeatmapsetDownloadOutcome::Success { .. }));
        assert_eq!(rate_hits.load(Ordering::SeqCst), 2);
        assert_eq!(missing_hits.load(Ordering::SeqCst), 1);
        server.join().unwrap();
    }

    #[tokio::test]
    async fn verify_archive_records_nonzero_duration_when_enabled() {
        use std::{
            io::{Read, Write},
            net::TcpListener,
            thread,
        };

        let zip_bytes = crate::validation::tests::minimal_zip_bytes_for_test();
        let len = zip_bytes.len();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let _ = stream.read(&mut request).unwrap();
            let header = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=99.osz\r\nContent-Length: {len}\r\n\r\n"
            );
            stream.write_all(header.as_bytes()).unwrap();
            stream.write_all(&zip_bytes).unwrap();
        });

        let client = reqwest::Client::new();
        let mirror = Mirror::custom(format!("http://{addr}/dl/{{id}}")).unwrap();
        let mirror_pool = MirrorPool::new(vec![mirror]);
        let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
        let dir = tempfile::tempdir().unwrap();

        let (outcome, _) = download_beatmapset(DownloadParams {
            beatmapset_id: 99,
            output_dir: dir.path(),
            client: &client,
            mirror_pool: &mirror_pool,
            verify_archive: true,
            verify_zip_eocd: true,
            progress_timeout: Duration::from_secs(1),
            callbacks: BeatmapsetDownloadCallbacks::default(),
            options: BeatmapsetDownloadOptions::default(),
            cancel_rx,
        })
        .await;

        match outcome {
            BeatmapsetDownloadOutcome::Success {
                verify_duration_us, ..
            } => assert!(
                verify_duration_us > 0,
                "verify_duration_us must be non-zero when verification runs (got {verify_duration_us}us)"
            ),
            other => panic!("expected Success outcome, got {other:?}"),
        }
        server.join().unwrap();
    }

    #[tokio::test]
    async fn backoff_cancelled_before_expiry() {
        use std::time::Instant;

        let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            let _ = cancel_tx.send(true);
        });

        let start = Instant::now();
        assert!(sleep_cancelable(Duration::from_secs(1), &cancel_rx).await);

        assert!(
            start.elapsed() < Duration::from_millis(200),
            "backoff should have been cut short by cancel signal"
        );
    }
}
