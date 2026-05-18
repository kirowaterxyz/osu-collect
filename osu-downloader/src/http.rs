//! HTTP client creation and configuration

use crate::Result;
use std::time::Duration;

/// Default connection timeout for beatmapset downloads (30 seconds)
pub const DEFAULT_DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default timeout for API requests (15 seconds)
#[cfg(feature = "collection")]
pub const DEFAULT_API_TIMEOUT: Duration = Duration::from_secs(15);

/// Create a configured HTTP client for downloading beatmapsets
pub fn create_download_client(user_agent: Option<String>) -> Result<reqwest::Client> {
    download_client_builder(user_agent)
        .build()
        .map_err(Into::into)
}

fn download_client_builder(user_agent: Option<String>) -> reqwest::ClientBuilder {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(DEFAULT_DOWNLOAD_CONNECT_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(5))
        .pool_max_idle_per_host(10);

    if let Some(ua) = user_agent {
        builder = builder.user_agent(ua);
    }

    builder
}

/// Create a configured HTTP client for API requests
#[cfg(feature = "collection")]
pub fn create_api_client() -> Result<reqwest::Client> {
    Ok(reqwest::Client::builder()
        .timeout(DEFAULT_API_TIMEOUT)
        .pool_max_idle_per_host(20)
        .build()?)
}
