use crate::{
    config::constants::{GB, KB, LOW_SPACE_THRESHOLD_BYTES, MB},
    utils::error::{AppError, Result},
};
use fs2::available_space;
use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};
use tokio::fs;

/// Replace the home directory prefix in `path` with `~`.
///
/// Returns `"~"` (a `&'static str` borrow) when `path` is exactly the home
/// directory, `"~/…"` (owned) when it is a subdirectory, and the result of
/// `to_string_lossy().into_owned()` (owned) for all other paths. Always
/// allocates for paths outside the home directory because `Path::to_string_lossy`
/// does not hand out a borrow tied to the caller's `path` argument.
pub fn pretty_path(path: impl AsRef<Path>) -> Cow<'static, str> {
    let path = path.as_ref();
    if let Some(home) = dirs::home_dir() {
        if path == home {
            return Cow::Borrowed("~");
        }
        if let Ok(stripped) = path.strip_prefix(&home) {
            let mut s = String::with_capacity(2 + stripped.as_os_str().len());
            s.push_str("~/");
            s.push_str(&stripped.to_string_lossy());
            return Cow::Owned(s);
        }
    }
    Cow::Owned(path.to_string_lossy().into_owned())
}

/// Expand a leading `~` or `~/` to the user's home directory.
///
/// Only the literal leading `~` followed by `/` or end-of-string is
/// substituted. Absolute paths, relative paths, and paths starting with `~`
/// followed by a non-`/` character are returned unchanged.
pub fn expand_tilde(path: &str) -> String {
    if path == "~" {
        return dirs::home_dir()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
    }
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        let mut s = home.to_string_lossy().into_owned();
        s.push('/');
        s.push_str(rest);
        return s;
    }
    path.to_string()
}

pub fn is_low_disk_space(path: &Path) -> bool {
    available_space(path).is_ok_and(|space| space < LOW_SPACE_THRESHOLD_BYTES)
}

/// Format `bytes` with a SI-1024 scale and the given unit suffix.
/// Use `"B"` for sizes (`"3.45 GB"`) or `"B/s"` for rates (`"3.45 GB/s"`).
pub fn format_bytes(bytes: u64, unit: &str) -> String {
    let bytes_f = bytes as f64;
    if bytes_f >= GB {
        format!("{:.2} G{unit}", bytes_f / GB)
    } else if bytes_f >= MB {
        format!("{:.1} M{unit}", bytes_f / MB)
    } else if bytes_f >= KB {
        format!("{:.0} K{unit}", bytes_f / KB)
    } else {
        format!("{bytes} {unit}")
    }
}

pub async fn prepare_directory(directory: &str) -> Result<PathBuf> {
    let expanded_path = PathBuf::from(expand_tilde(directory));

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

#[cfg(test)]
#[path = "../../tests/unit/utils_fs.rs"]
mod tests;
