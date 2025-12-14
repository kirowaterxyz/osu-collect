use crate::config::constants::{CONCURRENT_REQUESTS, MIRROR_CHECK_URLS, NEKOHA_API_BASE};
use futures_util::{StreamExt, stream};
use reqwest::Client;
use serde::Deserialize;
use std::{collections::HashSet, time::Duration};
use tracing::{debug, info, trace, warn};

const MIRROR_RETRIES: usize = 2;
const MAX_REDIRECTS: usize = 3;

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

/// Check beatmapsets availability across mirrors.
/// Uses ZIP magic byte verification to ensure actual archives are available.
pub async fn check_mirror_availability(
    client: &Client,
    beatmapset_ids: &[u32],
) -> MirrorAvailabilityResult {
    let results: Vec<(u32, bool)> = stream::iter(beatmapset_ids.iter().copied())
        .map(|id| {
            let client = client.clone();
            async move {
                let available = check_availability_on_any_mirror(&client, id).await;
                (id, available)
            }
        })
        .buffer_unordered(CONCURRENT_REQUESTS)
        .collect()
        .await;

    let mut available = HashSet::new();
    let mut unavailable = HashSet::new();

    for (id, is_available) in results {
        if is_available {
            available.insert(id);
        } else {
            unavailable.insert(id);
        }
    }

    info!(
        total = beatmapset_ids.len(),
        available = available.len(),
        unavailable = unavailable.len(),
        "Checked beatmapset availability on mirrors"
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

    let mut total_bytes: u64 = 0;
    let mut fetched_count: usize = 0;
    let mut missing_count: u32 = 0;

    for (_id, size_opt) in results {
        match size_opt {
            Some(size) => {
                total_bytes = total_bytes.saturating_add(size);
                fetched_count += 1;
            }
            None => {
                missing_count += 1;
            }
        }
    }

    debug!(
        total_bytes,
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
async fn check_availability_on_any_mirror(client: &Client, beatmapset_id: u32) -> bool {
    check_availability_on_urls(client, beatmapset_id, MIRROR_CHECK_URLS).await
}

#[doc(hidden)]
pub async fn check_availability_on_urls(
    client: &Client,
    beatmapset_id: u32,
    urls: &[&str],
) -> bool {
    for template in urls {
        let mut url = template.replace("{id}", &beatmapset_id.to_string());
        let mut redirects_remaining = MAX_REDIRECTS;
        let mut attempts: usize = 0;

        while attempts <= MIRROR_RETRIES {
            match probe_mirror(client, beatmapset_id, &url).await {
                ProbeResult::Available => return true,
                ProbeResult::RetryRedirect(next) => {
                    if redirects_remaining == 0 {
                        trace!(beatmapset_id, mirror = %url, "Redirect limit reached while probing mirror");
                        break;
                    }
                    redirects_remaining = redirects_remaining.saturating_sub(1);
                    url = next;
                    continue;
                }
                ProbeResult::RetryTransient => {
                    attempts = attempts.saturating_add(1);
                    continue;
                }
                ProbeResult::Rejected => break,
            }
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

    match resp.bytes().await {
        Ok(bytes) => {
            if bytes.len() >= 4 && bytes[0..4] == [0x50, 0x4B, 0x03, 0x04] {
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
