use crate::config::constants::{CONCURRENT_REQUESTS, NEKOHA_API_BASE};
use futures_util::{StreamExt, TryStreamExt, stream, stream::FuturesUnordered};
use reqwest::Client;
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};
use tracing::{debug, info, trace, warn};

const MIRROR_RETRIES: usize = 2;
const MAX_REDIRECTS: usize = 3;
const ZIP_MAGIC_LENGTH: usize = 4;

fn deserialize_string_to_u64<'de, D>(deserializer: D) -> Result<Option<u64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Number(u64),
        Null,
    }

    match StringOrNumber::deserialize(deserializer)? {
        StringOrNumber::String(s) => s
            .parse::<u64>()
            .map(Some)
            .map_err(|_| D::Error::custom("invalid u64 string")),
        StringOrNumber::Number(n) => Ok(Some(n)),
        StringOrNumber::Null => Ok(None),
    }
}

#[derive(Debug, Deserialize)]
struct BeatmapsetResponse {
    #[serde(default, deserialize_with = "deserialize_string_to_u64")]
    file_size: Option<u64>,
}

pub struct SizeFetchResult {
    pub total_bytes: u64,
    pub missing_count: u32,
}

/// Results from checking beatmapset availability on mirrors
pub struct MirrorAvailabilityResult {
    /// IDs of beatmapsets that are available on at least one mirror
    pub available: HashSet<u32>,
    /// IDs of beatmapsets that are unavailable on all mirrors
    pub unavailable: HashSet<u32>,
}

pub async fn check_mirror_availability(
    client: &Client,
    beatmapset_ids: &[u32],
    urls: &[&str],
    mut report_progress: impl FnMut(usize, usize),
) -> MirrorAvailabilityResult {
    let total = beatmapset_ids.len();
    let mut available = HashSet::new();
    let mut unavailable = HashSet::new();
    let mut checked = 0;

    let mut results = stream::iter(beatmapset_ids.iter().copied())
        .map(|id| {
            let client = client.clone();
            async move {
                let available = check_availability_on_urls(&client, id, urls).await;
                (id, available)
            }
        })
        .buffer_unordered(CONCURRENT_REQUESTS);

    while let Some((id, is_available)) = results.next().await {
        if is_available {
            available.insert(id);
        } else {
            unavailable.insert(id);
        }
        checked += 1;
        report_progress(checked, total);
    }

    info!(
        total,
        available = available.len(),
        unavailable = unavailable.len(),
        "checked beatmapset availability on mirrors"
    );

    MirrorAvailabilityResult {
        available,
        unavailable,
    }
}

pub async fn fetch_beatmapset_sizes(client: &Client, beatmapset_ids: &[u32]) -> SizeFetchResult {
    let results: Vec<(u32, Option<u64>)> = stream::iter(beatmapset_ids.iter().copied())
        .map(|id| {
            let client = client.clone();
            async move {
                let size = fetch_single_size(&client, id).await;
                (id, size)
            }
        })
        .buffer_unordered(CONCURRENT_REQUESTS)
        .collect()
        .await;

    let mut known_bytes: u64 = 0;
    let mut fetched_count: usize = 0;
    let mut missing_count: u32 = 0;

    for (_id, size_opt) in results {
        match size_opt {
            Some(size) => {
                known_bytes = known_bytes.saturating_add(size);
                fetched_count += 1;
            }
            None => {
                missing_count += 1;
            }
        }
    }

    let total_bytes = if missing_count > 0 && fetched_count > 0 {
        let average = known_bytes / fetched_count as u64;
        known_bytes.saturating_add(average.saturating_mul(missing_count as u64))
    } else {
        known_bytes
    };

    debug!(
        total_bytes,
        known_bytes,
        fetched = fetched_count,
        missing = missing_count,
        "Fetched beatmapset sizes from nekoha"
    );

    SizeFetchResult {
        total_bytes,
        missing_count,
    }
}

async fn fetch_single_size(client: &Client, beatmapset_id: u32) -> Option<u64> {
    let url = format!("{}/beatmapset/{}", NEKOHA_API_BASE, beatmapset_id);

    let response = match client.get(&url).send().await {
        Ok(resp) => resp,
        Err(err) => {
            warn!(beatmapset_id, error = %err, "Failed to fetch beatmapset size");
            return None;
        }
    };

    if !response.status().is_success() {
        return None;
    }

    match response.json::<BeatmapsetResponse>().await {
        Ok(data) => data.file_size,
        Err(err) => {
            warn!(beatmapset_id, error = %err, "Failed to parse beatmapset response");
            None
        }
    }
}

/// Check if a beatmapset is available on any mirror by verifying ZIP magic bytes.
/// HEAD requests alone are unreliable as some mirrors return 200 for error pages.
/// Probe one mirror URL, following redirects up to `MAX_REDIRECTS` and retrying
/// transient errors up to `MIRROR_RETRIES` times.
async fn probe_redirects(client: &Client, beatmapset_id: u32, template: &str) -> bool {
    let mut url = template.replace("{id}", &beatmapset_id.to_string());
    let mut redirects_remaining = MAX_REDIRECTS;
    let mut retries_remaining = MIRROR_RETRIES;

    loop {
        match probe_mirror(client, beatmapset_id, &url).await {
            ProbeResult::Available => return true,
            ProbeResult::RetryRedirect(next) => {
                if redirects_remaining == 0 {
                    trace!(beatmapset_id, mirror = %url, "redirect limit reached while probing mirror");
                    return false;
                }
                redirects_remaining = redirects_remaining.saturating_sub(1);
                url = next;
            }
            ProbeResult::RetryTransient => {
                if retries_remaining == 0 {
                    return false;
                }
                retries_remaining = retries_remaining.saturating_sub(1);
            }
            ProbeResult::Rejected => return false,
        }
    }
}

#[doc(hidden)]
pub async fn check_availability_on_urls(
    client: &Client,
    beatmapset_id: u32,
    urls: &[&str],
) -> bool {
    let mut probes: FuturesUnordered<_> = urls
        .iter()
        .map(|template| probe_redirects(client, beatmapset_id, template))
        .collect();

    while let Some(available) = probes.next().await {
        if available {
            return true;
        }
    }
    false
}

enum ProbeResult {
    Available,
    RetryRedirect(String),
    RetryTransient,
    Rejected,
}

async fn probe_mirror(client: &Client, beatmapset_id: u32, url: &str) -> ProbeResult {
    let result = client
        .get(url)
        .header("Range", "bytes=0-3")
        .header("Connection", "close")
        .timeout(Duration::from_secs(10))
        .send()
        .await;

    let resp = match result {
        Ok(resp) => resp,
        Err(err) => {
            if err.is_timeout() || err.is_connect() || err.is_request() {
                trace!(beatmapset_id, mirror = %url, error = %err, "Transient error while probing mirror");
                return ProbeResult::RetryTransient;
            }
            return ProbeResult::Rejected;
        }
    };

    let status = resp.status();
    if status.is_redirection() {
        if let Some(location) = resp.headers().get(reqwest::header::LOCATION)
            && let Ok(location_str) = location.to_str()
        {
            if let Ok(next_url) = resp.url().join(location_str) {
                trace!(beatmapset_id, mirror = %url, redirect = %next_url, "Following redirect while probing mirror");
                return ProbeResult::RetryRedirect(next_url.to_string());
            }
            trace!(beatmapset_id, mirror = %url, redirect = %location_str, "Following redirect while probing mirror");
            return ProbeResult::RetryRedirect(location_str.to_string());
        }
        return ProbeResult::RetryTransient;
    }

    if status.is_server_error() {
        trace!(beatmapset_id, mirror = %url, status = %status, "Retrying after server error");
        return ProbeResult::RetryTransient;
    }

    if !status.is_success() {
        trace!(beatmapset_id, mirror = %url, status = %status, "Rejected mirror due to status code");
        return ProbeResult::Rejected;
    }

    if let Some(content_type) = resp.headers().get("content-type")
        && let Ok(ct) = content_type.to_str()
    {
        let ct_lower = ct.to_ascii_lowercase();
        if ct_lower.contains("text/html") || ct_lower.contains("application/json") {
            trace!(beatmapset_id, mirror = %url, content_type = %ct, "Rejected: error page content type");
            return ProbeResult::Rejected;
        }
    }

    match read_probe_prefix(resp).await {
        Ok(bytes) => {
            if bytes == [0x50, 0x4B, 0x03, 0x04] {
                trace!(beatmapset_id, mirror = %url, "Verified available with ZIP magic");
                ProbeResult::Available
            } else {
                trace!(beatmapset_id, mirror = %url, "Rejected: invalid ZIP magic bytes");
                ProbeResult::Rejected
            }
        }
        Err(err) => {
            trace!(beatmapset_id, mirror = %url, error = %err, "Retrying after read failure while probing mirror");
            ProbeResult::RetryTransient
        }
    }
}

async fn read_probe_prefix(resp: reqwest::Response) -> Result<Vec<u8>, reqwest::Error> {
    let mut bytes = Vec::with_capacity(ZIP_MAGIC_LENGTH);
    let mut stream = resp.bytes_stream();

    while bytes.len() < ZIP_MAGIC_LENGTH {
        let Some(chunk) = stream.try_next().await? else {
            break;
        };
        let remaining = ZIP_MAGIC_LENGTH - bytes.len();
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }

    Ok(bytes)
}
