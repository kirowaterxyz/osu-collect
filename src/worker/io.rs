use crate::utils::{AppError, Result};
use std::path::Path;

pub use osu_downloader::{ArchiveValidation, ArchiveValidationOptions, ArchiveValidationResult};

pub async fn ensure_valid_archive(path: &Path, mode: ArchiveValidation) -> Result<()> {
    osu_downloader::ensure_valid_archive(path, mode)
        .await
        .map_err(to_app_error)
}

pub async fn validate_archive(
    path: &Path,
    options: ArchiveValidationOptions,
) -> Result<ArchiveValidationResult> {
    osu_downloader::validate_archive(path, options)
        .await
        .map_err(to_app_error)
}

fn to_app_error(err: osu_downloader::Error) -> AppError {
    AppError::other_dynamic(err.to_string().into_boxed_str())
}
