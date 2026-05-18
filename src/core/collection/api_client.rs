use super::Collection;
use crate::{
    config::constants::API_MAX_RETRIES,
    utils::{AppError, Result},
};
use osu_downloader::collection::{CollectionClient, CollectionError};
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
    client: CollectionClient,
}

impl HttpCollectionService {
    pub fn new(client: CollectionClient) -> Self {
        Self { client }
    }

    pub fn create() -> Result<Self> {
        Ok(Self::new(CollectionClient::new()))
    }
}

impl CollectionService for HttpCollectionService {
    async fn fetch_collection(&self, collection_id: u32) -> Result<Collection> {
        fetch_collection(&self.client, collection_id).await
    }
}

pub async fn fetch_collection(client: &CollectionClient, collection_id: u32) -> Result<Collection> {
    let mut last_error: Option<AppError> = None;

    for attempt in 1..=API_MAX_RETRIES {
        match client.fetch(collection_id).await {
            Ok(collection) => return Ok(collection),
            Err(CollectionError::RateLimited { retry_after }) => {
                let delay = retry_after
                    .unwrap_or(Duration::from_secs(30))
                    .min(Duration::from_secs(60));
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
            Err(CollectionError::NotFound) => {
                return Err(AppError::api_dynamic(
                    format!("collection {collection_id} not found (404)").into_boxed_str(),
                ));
            }
            Err(CollectionError::Status(status)) => {
                return Err(AppError::api_dynamic(
                    format!("failed to fetch collection: HTTP {status}").into_boxed_str(),
                ));
            }
            Err(CollectionError::Network(err)) => {
                let mapped = if err.is_timeout() {
                    AppError::api("request timed out after 30 seconds")
                } else if err.is_connect() {
                    AppError::api("failed to connect to osucollector.com")
                } else {
                    AppError::from(err)
                };
                let should_retry = matches!(mapped, AppError::Network(_));
                if should_retry && attempt < API_MAX_RETRIES {
                    let delay_secs = 2_u64.pow((attempt - 1) as u32);
                    warn!(
                        attempt,
                        remaining_attempts = API_MAX_RETRIES - attempt,
                        delay_secs,
                        error = %mapped,
                        "fetch collection attempt failed; retrying"
                    );
                    sleep(Duration::from_secs(delay_secs)).await;
                    last_error = Some(mapped);
                } else {
                    return Err(mapped);
                }
            }
            Err(CollectionError::Parse(err)) => {
                return Err(AppError::api_dynamic(
                    format!("failed to parse collection JSON: {err}").into_boxed_str(),
                ));
            }
            Err(err @ CollectionError::InvalidUrl(_)) => {
                debug!(error = %err, "invalid collection URL");
                return Err(AppError::api_dynamic(err.to_string().into_boxed_str()));
            }
        }
    }

    Err(last_error.unwrap_or_else(|| AppError::api("all retry attempts failed")))
}
