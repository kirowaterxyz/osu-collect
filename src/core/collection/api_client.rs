use super::model::Collection;
use crate::{
    config::constants::API_MAX_RETRIES,
    utils::{AppError, Result},
};
use std::time::Duration;
use tokio::time::sleep;
use tracing::{debug, warn};

pub trait CollectionService: Send + Sync {
    fn fetch_collection(
        &self,
        collection_id: u32,
    ) -> impl std::future::Future<Output = Result<Collection>> + Send;
}

pub struct HttpCollectionService {
    client: reqwest::Client,
}

impl HttpCollectionService {
    pub fn new(client: reqwest::Client) -> Self {
        Self { client }
    }

    pub fn builder() -> HttpCollectionServiceBuilder {
        HttpCollectionServiceBuilder::new()
    }
}

impl CollectionService for HttpCollectionService {
    async fn fetch_collection(&self, collection_id: u32) -> Result<Collection> {
        fetch_collection(&self.client, collection_id).await
    }
}

pub struct HttpCollectionServiceBuilder;

impl Default for HttpCollectionServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpCollectionServiceBuilder {
    pub fn new() -> Self {
        Self
    }

    pub fn build(self) -> Result<HttpCollectionService> {
        let client = osu_downloader::http::create_api_client().map_err(|e| {
            AppError::other_dynamic(format!("failed to create API client: {e}").into_boxed_str())
        })?;
        Ok(HttpCollectionService::new(client))
    }
}

pub async fn fetch_collection(client: &reqwest::Client, collection_id: u32) -> Result<Collection> {
    let url = format!("https://osucollector.com/api/collections/{collection_id}");
    let mut last_error = None;

    for attempt in 1..=API_MAX_RETRIES {
        match try_fetch_collection(client, &url, collection_id).await {
            Ok(collection) => return Ok(collection),
            Err(FetchError::RateLimited(retry_after)) => {
                let delay = retry_after.min(Duration::from_secs(60));
                warn!(
                    attempt,
                    delay_secs = delay.as_secs(),
                    "rate limited by osucollector.com (429); waiting before retry"
                );
                sleep(delay).await;
                last_error = Some(AppError::api(
                    "rate limited by osucollector.com (429). please try again later.",
                ));
            }
            Err(FetchError::App(err)) => {
                let should_retry = matches!(err, AppError::Network(_));

                if should_retry && attempt < API_MAX_RETRIES {
                    let delay_secs = 2_u64.pow((attempt - 1) as u32);
                    warn!(
                        attempt,
                        remaining_attempts = API_MAX_RETRIES - attempt,
                        delay_secs,
                        error = %err,
                        "fetch collection attempt failed; retrying"
                    );
                    sleep(Duration::from_secs(delay_secs)).await;
                    last_error = Some(err);
                } else {
                    return Err(err);
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::api("all retry attempts failed")))
}

enum FetchError {
    RateLimited(Duration),
    App(AppError),
}

impl From<AppError> for FetchError {
    fn from(e: AppError) -> Self {
        Self::App(e)
    }
}

async fn try_fetch_collection(
    client: &reqwest::Client,
    url: &str,
    collection_id: u32,
) -> std::result::Result<Collection, FetchError> {
    let response = client.get(url).send().await.map_err(|err| {
        FetchError::App(if err.is_timeout() {
            AppError::api("request timed out after 30 seconds")
        } else if err.is_connect() {
            AppError::api("failed to connect to osucollector.com")
        } else {
            AppError::from(err)
        })
    })?;

    let status = response.status();

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        let retry_after = response
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse::<u64>().ok())
            .map(Duration::from_secs)
            .unwrap_or(Duration::from_secs(30));
        debug!(
            retry_after_secs = retry_after.as_secs(),
            "got 429 from osucollector"
        );
        return Err(FetchError::RateLimited(retry_after));
    }

    if status == reqwest::StatusCode::NOT_FOUND {
        return Err(FetchError::App(AppError::api_dynamic(
            format!("collection {collection_id} not found (404)").into_boxed_str(),
        )));
    }

    if !status.is_success() {
        return Err(FetchError::App(AppError::api_dynamic(
            format!("failed to fetch collection: HTTP {status}").into_boxed_str(),
        )));
    }

    let collection: Collection = response.json().await.map_err(|err| {
        FetchError::App(AppError::api_dynamic(
            format!("failed to parse collection JSON: {err}").into_boxed_str(),
        ))
    })?;

    Ok(collection)
}
