use crate::{
    config::constants::{GB, KB, LOW_SPACE_THRESHOLD_BYTES, MB},
    utils::error::{AppError, Result},
};
use fs2::available_space;
use std::path::{Path, PathBuf};
use tokio::fs;

pub fn is_low_disk_space(path: &Path) -> bool {
    available_space(path).is_ok_and(|space| space < LOW_SPACE_THRESHOLD_BYTES)
}

pub fn format_bytes(bytes: u64) -> String {
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.2} GB", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} MB", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.0} KB", bytes_f / KB)
    } else {
        format!("{bytes} B")
    }
}

pub async fn prepare_directory(directory: &str) -> Result<PathBuf> {
    let expanded_path = if let Some(stripped) = directory.strip_prefix("~/") {
        if let Some(home_dir) = dirs::home_dir() {
            home_dir.join(stripped)
        } else {
            PathBuf::from(directory)
        }
    } else {
        PathBuf::from(directory)
    };

    if !expanded_path.exists() {
        fs::create_dir_all(&expanded_path).await.map_err(|err| {
            let message = format!(
                "Failed to create directory '{}': {}",
                expanded_path.display(),
                err
            );
            AppError::filesystem_source(err, message.into_boxed_str())
        })?;
    }

    let metadata = fs::metadata(&expanded_path).await?;
    if !metadata.is_dir() {
        return Err(AppError::filesystem_context(
            format!("Path '{}' is not a directory", expanded_path.display()).into_boxed_str(),
        ));
    }

    let test_file = expanded_path.join(".write_test");
    match fs::File::create(&test_file).await {
        Ok(_) => {
            let _ = fs::remove_file(&test_file).await;
            let canonical_path = fs::canonicalize(&expanded_path).await?;
            Ok(canonical_path)
        }
        Err(err) => {
            let message = format!(
                "Directory '{}' is not writable: {}",
                expanded_path.display(),
                err
            );
            Err(AppError::filesystem_source(err, message.into_boxed_str()))
        }
    }
}
