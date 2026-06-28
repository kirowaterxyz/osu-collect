//! Per-beatmapset download state machine.
//!
//! Internal to the crate. `batch.rs` calls [`download_beatmapset`] for each item; results are
//! translated into the public [`DownloadEvent`](crate::DownloadEvent) stream there.

use crate::{
    config::{TRANSIENT_RETRY_ATTEMPTS, TRANSIENT_RETRY_BASE_DELAY},
    downloader::OnExists,
    event::{Skip, Status},
    mirrors::{Mirror, MirrorKind, MirrorPool, MirrorRef},
    output_entry::parse_beatmapset_filename,
    validation::{self, ArchiveValidation},
    worker::stream_download,
};
use std::{
    collections::HashSet,
    future::{Future, pending},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::time::sleep;
use tracing::{debug, trace};

const SIZE_PROBE_REDIRECT_LIMIT: usize = 4;

const MIME_OSU_BEATMAP_ARCHIVE: &str = "application/x-osu-beatmap-archive";
const MIME_OCTET_STREAM: &str = "application/octet-stream";
const MIME_BINARY_OCTET_STREAM: &str = "binary/octet-stream";
const MIME_ZIP: &str = "application/zip";
const MIME_X_ZIP_COMPRESSED: &str = "application/x-zip-compressed";

#[derive(Clone, Default)]
pub(crate) struct BeatmapsetDownloadCallbacks {
    pub(crate) progress: Option<Arc<dyn Fn(u64, u64) + Send + Sync>>,
    pub(crate) status: Option<Arc<dyn Fn(Status) + Send + Sync>>,
}

#[derive(Debug, Clone)]
pub(crate) enum BeatmapsetDownloadOutcome {
    Success {
        filename: String,
        hash: String,
        mirror: MirrorRef,
        size_bytes: u64,
        verify_duration_us: u64,
    },
    Skipped {
        reason: Skip,
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
    pub(crate) archive_validation: ArchiveValidation,
    pub(crate) progress_timeout: Duration,
    pub(crate) sanitize_filenames: bool,
    pub(crate) on_exists: OnExists,
    pub(crate) callbacks: BeatmapsetDownloadCallbacks,
    pub(crate) cancel_rx: tokio::sync::watch::Receiver<bool>,
    /// Edge signal to abandon a map currently parked on a rate-limit cooldown.
    /// Shared across the whole session; `notify_waiters` wakes only the maps
    /// waiting at that instant (see [`download_beatmapset`]'s cooldown race).
    pub(crate) skip: Arc<tokio::sync::Notify>,
    /// Auto-skip budget: once this map's summed cooldown wait reaches it, the
    /// map is skipped without the manual `skip` signal. `None` waits forever.
    pub(crate) rate_limit_skip_after: Option<Duration>,
}

/// Sanitize a raw filename to be safe for use as an `.osz` archive name.
///
/// Replaces filesystem-unsafe characters (`/`, `\\`, `:`, `*`, `?`, `"`, `<`, `>`, `|`)
/// with `_`. Falls back to `<beatmapset_id>.osz` if the input is missing, empty,
/// a relative-path token (`.`, `..`), starts with `.`, or otherwise fails a
/// path-traversal safety check.
///
/// The library calls this automatically on every download unless
/// [`DownloaderBuilder::sanitize_filenames`](crate::DownloaderBuilder::sanitize_filenames)
/// is disabled. Exposed publicly so callers can reuse the same logic when they
/// are choosing filenames outside the library.
pub fn sanitize_filename(raw: Option<&str>, beatmapset_id: u32) -> std::borrow::Cow<'_, str> {
    let fallback = || std::borrow::Cow::Owned(format!("{beatmapset_id}.osz"));

    let Some(name) = raw else {
        return fallback();
    };

    // All forbidden chars are single-byte ASCII and cannot appear as continuation
    // bytes in multi-byte UTF-8 sequences, so byte-level scanning and replacement
    // is safe and avoids the char decode overhead entirely.
    #[inline]
    fn is_forbidden(b: u8) -> bool {
        matches!(
            b,
            b'/' | b'\\' | b':' | b'*' | b'?' | b'"' | b'<' | b'>' | b'|'
        )
    }

    let needs_replacement = name.bytes().any(is_forbidden);

    let sanitized: std::borrow::Cow<'_, str> = if needs_replacement {
        let mut out = Vec::with_capacity(name.len());
        for b in name.bytes() {
            out.push(if is_forbidden(b) { b'_' } else { b });
        }
        // SAFETY: we only replace ASCII bytes with `_` (also ASCII); all non-ASCII
        // bytes pass through unchanged, preserving valid UTF-8.
        std::borrow::Cow::Owned(unsafe { String::from_utf8_unchecked(out) })
    } else {
        std::borrow::Cow::Borrowed(name)
    };

    let is_safe = !sanitized.is_empty()
        && sanitized != "."
        && sanitized != ".."
        && !sanitized.starts_with('.')
        && std::path::Path::new(sanitized.as_ref()).file_name()
            == Some(std::ffi::OsStr::new(sanitized.as_ref()));

    if is_safe { sanitized } else { fallback() }
}

/// Extract filename from Content-Disposition header.
///
/// Handles `filename*=UTF-8''...` and `filename="..."`.
pub(crate) fn parse_content_disposition(header_value: &str) -> Option<String> {
    let mut filename = None;
    let mut extended_filename = None;

    let parts = ContentDispositionParts {
        rest: header_value,
        done: false,
    };
    for part in parts {
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

/// Allocation-free iterator over `;`-separated Content-Disposition parts.
struct ContentDispositionParts<'a> {
    rest: &'a str,
    done: bool,
}

impl<'a> Iterator for ContentDispositionParts<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<&'a str> {
        if self.done {
            return None;
        }

        let mut quoted = false;
        let mut escaped = false;

        for (index, ch) in self.rest.char_indices() {
            if escaped {
                escaped = false;
                continue;
            }
            match ch {
                '\\' if quoted => escaped = true,
                '"' => quoted = !quoted,
                ';' if !quoted => {
                    let part = self.rest[..index].trim();
                    self.rest = &self.rest[index + 1..];
                    return Some(part);
                }
                _ => {}
            }
        }

        self.done = true;
        Some(self.rest.trim())
    }
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
        ExistingCheck::Exists => match params.on_exists {
            OnExists::Skip => {
                return (
                    BeatmapsetDownloadOutcome::Skipped {
                        reason: Skip::AlreadyExists,
                    },
                    0,
                );
            }
            OnExists::Overwrite => {}
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
    // Time this map has actually spent parked on rate-limit cooldowns, summed
    // across mirror passes. Drives the auto-skip budget below.
    let mut cumulative_rate_limit = Duration::ZERO;

    let mirrors = params.mirror_pool.mirrors();
    let mirror_count = mirrors.len();
    // Round-robin the initial mirror per map: each download starts at a rotating
    // slot and wraps, so concurrent maps spread across mirrors instead of all
    // hammering slot 0 first.
    let start = if mirror_count == 0 {
        0
    } else {
        params.mirror_pool.next_round_robin_start() % mirror_count
    };
    let mut pending: Vec<usize> = (0..mirror_count)
        .map(|offset| (start + offset) % mirror_count)
        .collect();

    while !pending.is_empty() {
        if *cancel_rx.borrow_and_update() {
            return (BeatmapsetDownloadOutcome::Aborted, total_attempts);
        }

        let mut deferred_rate_limited = Vec::new();
        for idx in &pending {
            let mirror = &mirrors[*idx];
            if *cancel_rx.borrow_and_update() {
                return (BeatmapsetDownloadOutcome::Aborted, total_attempts);
            }

            match try_mirror_retry(mirror, *idx, &params, &mut total_attempts).await {
                MirrorAttempt::Done(outcome) => return (outcome, total_attempts),
                MirrorAttempt::NotFound => {
                    all_transient = false;
                    // Key the "tried and missing" set by slot, not kind: two
                    // custom mirrors share `Custom`, so kind-keying would let a
                    // collection that 404s on every mirror escape the
                    // all-missing → skip detection below.
                    not_found.insert(*idx);
                    last_error = Some(failed(Some(mirror.kind()), "not found (404)"));
                }
                MirrorAttempt::RateLimited => {
                    all_transient = false;
                    deferred_rate_limited.push(*idx);
                    params.mirror_pool.mark_rate_limited(*idx);
                    last_error = Some(failed(Some(mirror.kind()), "rate limited"));
                }
                MirrorAttempt::Transient(reason) => {
                    last_transient_reason = reason.clone();
                    last_error = Some(failed(Some(mirror.kind()), reason));
                }
                MirrorAttempt::Definitive(reason) => {
                    all_transient = false;
                    last_error = Some(failed(Some(mirror.kind()), reason));
                }
            }
        }

        if deferred_rate_limited.is_empty() {
            break;
        }

        let wait_duration = deferred_rate_limited
            .iter()
            .filter_map(|&idx| params.mirror_pool.penalty_remaining(idx))
            .min()
            .unwrap_or(Duration::ZERO);

        emit_status(
            &params.callbacks,
            Status::RateLimited {
                cooldown: wait_duration,
            },
        );

        if !wait_duration.is_zero() {
            // Cap the sleep at the remaining auto-skip budget so a budget
            // smaller than a mirror's cooldown is honored to the second instead
            // of overshooting to a full cooldown. `None` budget never caps, and
            // a budget larger than the cooldown behaves exactly as before.
            let (sleep_for, budget_exhausted) = match params
                .rate_limit_skip_after
                .map(|budget| budget.saturating_sub(cumulative_rate_limit))
            {
                Some(remaining) if remaining <= wait_duration => (remaining, true),
                _ => (wait_duration, false),
            };
            match wait_rate_limit(sleep_for, &cancel_rx, &params.skip).await {
                RateLimitWait::Cancelled => {
                    return (BeatmapsetDownloadOutcome::Aborted, total_attempts);
                }
                RateLimitWait::Skipped => {
                    return (
                        BeatmapsetDownloadOutcome::Skipped {
                            reason: Skip::RateLimitSkipped,
                        },
                        total_attempts,
                    );
                }
                RateLimitWait::Elapsed => {
                    // A cancel/skip returns above without counting; only a slept
                    // slice accrues. When that slice was the budget remainder the
                    // map has waited out its budget — auto-skip exactly as the
                    // manual `s` press does.
                    cumulative_rate_limit += sleep_for;
                    if budget_exhausted {
                        return (
                            BeatmapsetDownloadOutcome::Skipped {
                                reason: Skip::RateLimitSkipped,
                            },
                            total_attempts,
                        );
                    }
                }
            }
        }

        pending = deferred_rate_limited;
    }

    if not_found.len() == mirror_count && mirror_count > 0 {
        return (
            BeatmapsetDownloadOutcome::Skipped {
                reason: Skip::UnavailableOnMirrors,
            },
            total_attempts,
        );
    }

    if all_transient && !last_transient_reason.is_empty() {
        // A map that ever parked on a rate-limit set `all_transient = false`, so
        // reaching here means no cooldown was waited and the local budget
        // accumulator is zero — it never leaks across `process_one`'s
        // network-retry re-entry of this function.
        debug_assert!(cumulative_rate_limit.is_zero());
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
        if parse_beatmapset_filename(&name) == Some(beatmapset_id) {
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
    idx: usize,
    params: &DownloadParams<'_>,
    total_attempts: &mut u32,
) -> MirrorAttempt {
    let mut retry = 0u32;

    loop {
        if *params.cancel_rx.borrow() {
            return MirrorAttempt::Done(BeatmapsetDownloadOutcome::Aborted);
        }

        *total_attempts += 1;
        let outcome = try_mirror_once(mirror, idx, params).await;

        match outcome {
            MirrorAttempt::Transient(reason) if retry + 1 < TRANSIENT_RETRY_ATTEMPTS => {
                retry += 1;
                let backoff = TRANSIENT_RETRY_BASE_DELAY * (1u32 << (retry - 1));
                trace!(
                    beatmapset_id = params.beatmapset_id,
                    mirror = mirror.kind().label(),
                    retry,
                    reason = %reason,
                    "retrying mirror after transient error"
                );
                emit_status(
                    &params.callbacks,
                    Status::RetryingTransient {
                        mirror: mirror.mirror_ref(),
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

async fn try_mirror_once(
    mirror: &Mirror,
    idx: usize,
    params: &DownloadParams<'_>,
) -> MirrorAttempt {
    emit_status(
        &params.callbacks,
        Status::Contacting {
            mirror: mirror.mirror_ref(),
        },
    );

    // Proactive client-side spacing: gate every attempt to this mirror slot so
    // concurrent workers (and retries) can't burst past its per-kind minimum
    // interval — 100 ms for most mirrors, 1 s for the osu! official API. The
    // sleep is cancelable so a pending throttle never blocks a cancel.
    if run_cancelable(params.mirror_pool.throttle(idx), &params.cancel_rx)
        .await
        .is_none()
    {
        return MirrorAttempt::Done(BeatmapsetDownloadOutcome::Aborted);
    }

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
            return MirrorAttempt::Transient("connection timeout".to_string());
        }
        Err(err) if err.is_connect() => {
            return MirrorAttempt::Transient("connection failed".to_string());
        }
        Err(err) => {
            return MirrorAttempt::Transient(format!(
                "request failed on {}: {err}",
                mirror.kind().label()
            ));
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
                trace!(beatmapset_id = params.beatmapset_id, mirror = %mirror.kind().label(), error = %err, "failed to probe download size");
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
        && !is_archive_content_type(content_type)
    {
        return Err(format!(
            "unexpected content type '{content_type}' from {}",
            mirror.kind().label()
        ));
    }

    let filename = extract_filename(&response, params.beatmapset_id);
    let sanitized_filename = if params.sanitize_filenames {
        sanitize_filename(Some(&filename), params.beatmapset_id).into_owned()
    } else {
        filename
    };
    let output_path = params.output_dir.join(&sanitized_filename);

    if let Some(metadata_result) =
        run_cancelable(tokio::fs::metadata(&output_path), &params.cancel_rx).await
    {
        match metadata_result {
            Ok(_) => match params.on_exists {
                OnExists::Skip => {
                    return Ok(BeatmapsetDownloadOutcome::Skipped {
                        reason: Skip::AlreadyExists,
                    });
                }
                OnExists::Overwrite => {
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
        Status::Downloading {
            mirror: mirror.mirror_ref(),
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

    if let Some(expected) = content_length
        && stream.bytes_written < expected
    {
        let _ = tokio::fs::remove_file(&stream.temp_path).await;
        return Err(format!(
            "download incomplete from {} (received {} of {} bytes)",
            mirror.kind().label(),
            stream.bytes_written,
            expected
        ));
    }

    emit_status(
        &params.callbacks,
        Status::Verifying {
            mirror: mirror.mirror_ref(),
        },
    );

    let verify_start = Instant::now();
    if let Some(validate_result) = run_cancelable(
        validation::ensure_valid_archive(&stream.temp_path, params.archive_validation),
        &params.cancel_rx,
    )
    .await
    {
        if let Err(err) = validate_result {
            let _ = tokio::fs::remove_file(&stream.temp_path).await;
            return Err(format!(
                "{} returned an invalid archive: {err}",
                mirror.kind().label()
            ));
        }
    } else {
        let _ = tokio::fs::remove_file(&stream.temp_path).await;
        return Ok(BeatmapsetDownloadOutcome::Aborted);
    }
    let verify_duration_us = verify_start.elapsed().as_micros() as u64;

    match finalize_download(&stream.temp_path, &output_path, &params.cancel_rx).await {
        FinalizeResult::Done => {}
        FinalizeResult::AlreadyExists => {
            return Ok(BeatmapsetDownloadOutcome::Skipped {
                reason: Skip::AlreadyExists,
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
        mirror: mirror.mirror_ref(),
        size_bytes: stream.bytes_written,
        verify_duration_us,
    })
}

fn is_archive_content_type(raw: &str) -> bool {
    let mime = raw.split(';').next().map(str::trim).unwrap_or("");
    [
        MIME_OSU_BEATMAP_ARCHIVE,
        MIME_OCTET_STREAM,
        MIME_BINARY_OCTET_STREAM,
        MIME_ZIP,
        MIME_X_ZIP_COMPRESSED,
    ]
    .iter()
    .any(|&known| mime.eq_ignore_ascii_case(known))
}

fn extract_filename(response: &reqwest::Response, beatmapset_id: u32) -> String {
    let mut filename = response
        .headers()
        .get(reqwest::header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_content_disposition)
        .unwrap_or_else(|| format!("{beatmapset_id}.osz"));

    if !filename
        .rsplit_once('.')
        .is_some_and(|(_, ext)| ext.eq_ignore_ascii_case("osz"))
    {
        filename.push_str(".osz");
    }
    filename
}

pub(crate) async fn finalize_download(
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

pub(crate) enum FinalizeResult {
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

fn emit_status(callbacks: &BeatmapsetDownloadCallbacks, event: Status) {
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

enum RateLimitWait {
    Elapsed,
    Cancelled,
    Skipped,
}

/// Wait out a rate-limit cooldown, racing it against session cancellation and a
/// caller skip request. `notify_waiters` wakes only the maps parked here at the
/// instant of the press, so one skip drops exactly the currently-cooling maps
/// and leaves later rate-limits untouched. Cancel wins over skip (biased).
async fn wait_rate_limit(
    duration: Duration,
    cancel_rx: &tokio::sync::watch::Receiver<bool>,
    skip: &tokio::sync::Notify,
) -> RateLimitWait {
    let mut cancel_rx = cancel_rx.clone();
    tokio::select! {
        biased;
        _ = wait_until_cancelled(&mut cancel_rx) => RateLimitWait::Cancelled,
        _ = skip.notified() => RateLimitWait::Skipped,
        _ = sleep(duration) => RateLimitWait::Elapsed,
    }
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
#[path = "../tests/download.rs"]
mod tests;
