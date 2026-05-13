//! HTTP client creation and configuration

use crate::Result;
use std::time::Duration;

/// Default connection timeout for beatmapset downloads (30 seconds)
pub const DEFAULT_DOWNLOAD_CONNECT_TIMEOUT: Duration = Duration::from_secs(30);

/// Default total timeout for beatmapset download requests (5 minutes)
pub const DEFAULT_DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(300);

/// Default timeout for API requests (15 seconds)
#[cfg(feature = "collection")]
pub const DEFAULT_API_TIMEOUT: Duration = Duration::from_secs(15);

/// Create a configured HTTP client for downloading beatmapsets
pub fn create_download_client(user_agent: Option<String>) -> Result<reqwest::Client> {
    download_client_builder(user_agent, DEFAULT_DOWNLOAD_TIMEOUT)
        .build()
        .map_err(Into::into)
}

fn download_client_builder(
    user_agent: Option<String>,
    timeout: Duration,
) -> reqwest::ClientBuilder {
    let mut builder = reqwest::Client::builder()
        .connect_timeout(DEFAULT_DOWNLOAD_CONNECT_TIMEOUT)
        .timeout(timeout)
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
        .pool_max_idle_per_host(5)
        .build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_download_client() {
        let client = create_download_client(None);
        assert!(client.is_ok());
    }

    #[test]
    fn test_create_download_client_with_user_agent() {
        let client = create_download_client(Some("test-agent".to_string()));
        assert!(client.is_ok());
    }

    #[tokio::test]
    async fn download_client_times_out_waiting_for_headers() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.unwrap();
            tokio::time::sleep(Duration::from_secs(5)).await;
        });

        let client = download_client_builder(None, Duration::from_millis(50))
            .build()
            .unwrap();
        let result = client.get(format!("http://{address}")).send().await;

        assert!(result.is_err_and(|err| err.is_timeout()));
        server.abort();
    }

    #[cfg(feature = "collection")]
    #[test]
    fn test_create_api_client() {
        let client = create_api_client();
        assert!(client.is_ok());
    }
}
