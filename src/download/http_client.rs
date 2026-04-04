use crate::utils::{AppError, Result};

pub fn download_client() -> Result<reqwest::Client> {
    osu_downloader::http::create_download_client(None).map_err(|e| {
        AppError::other_dynamic(format!("Failed to create download client: {e}").into_boxed_str())
    })
}

pub fn api_client() -> Result<reqwest::Client> {
    osu_downloader::http::create_api_client().map_err(|e| {
        AppError::other_dynamic(format!("Failed to create API client: {e}").into_boxed_str())
    })
}
