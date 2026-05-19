use reqwest::Client;
use semver::Version;
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use thiserror::Error;
use tokio::{fs, io::AsyncWriteExt};
use tracing::{debug, info, warn};

use sha2::{Digest, Sha256};

use crate::config::constants::{AUTO_UPDATE_TIMEOUT, RELEASES_URL};

pub async fn check_and_apply<F>(on_update_found: F) -> Result<Option<String>, AutoUpdateError>
where
    F: FnOnce() + Send,
{
    let client = Client::builder()
        .user_agent(format!("osu-collect/{}", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(15))
        .build()?;

    check_release(&client, RELEASES_URL, on_update_found, |asset| async move {
        apply_update(&asset).await
    })
    .await
}

#[doc(hidden)]
pub async fn check_release<F, A, Fut>(
    client: &Client,
    releases_url: &str,
    on_update_found: F,
    applier: A,
) -> Result<Option<String>, AutoUpdateError>
where
    F: FnOnce() + Send,
    A: FnOnce(DownloadedAsset) -> Fut,
    Fut: Future<Output = Result<(), AutoUpdateError>> + Send,
{
    let Some(target_asset) = target_asset_name() else {
        debug!("auto-update skipped: unsupported platform");
        return Ok(None);
    };

    let release: ReleaseResponse = client
        .get(releases_url)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    let current_version = Version::parse(env!("CARGO_PKG_VERSION"))?;
    let latest_version = parse_release_version(&release)
        .ok_or_else(|| AutoUpdateError::UnparseableVersion(release.tag_name.clone()))?;

    if latest_version <= current_version {
        debug!(?latest_version, ?current_version, "no updates available");
        return Ok(None);
    }

    on_update_found();

    let asset = release
        .assets
        .iter()
        .find(|asset| asset.name == target_asset)
        .ok_or_else(|| AutoUpdateError::AssetMissing(target_asset.to_string()))?;

    info!(release = %release.name, "Downloading newer release");
    let downloaded = download_asset(client, asset, AUTO_UPDATE_TIMEOUT).await?;

    let expected_checksum = match fetch_asset_checksum(client, asset, &release.assets).await {
        Ok(checksum) => checksum,
        Err(err) => {
            let _ = fs::remove_file(&downloaded.path).await;
            return Err(err);
        }
    };

    verify_checksum(&downloaded, &expected_checksum).await?;

    applier(downloaded).await?;

    let message = format!("Application updated to {}, please restart", release.name);
    Ok(Some(message))
}

pub fn spawn_background_update() {
    let handle = spawn_update_task(|| check_and_apply(print_update_banner));
    drop(handle);
}

pub fn spawn_update_task<Fut>(
    update_fn: impl FnOnce() -> Fut + Send + 'static,
) -> tokio::task::JoinHandle<()>
where
    Fut: Future<Output = Result<Option<String>, AutoUpdateError>> + Send + 'static,
{
    tokio::spawn(async move {
        match update_fn().await {
            Ok(Some(message)) => {
                info!(%message, "Auto-update applied");
            }
            Ok(None) => {}
            Err(err) => {
                warn!(error = %err, "Auto-update failed; new version may be available");
            }
        }
    })
}

fn print_update_banner() {
    println!("{}", update_banner());
}

#[doc(hidden)]
pub fn update_banner() -> &'static str {
    "\u{1b}[32mDownloading update...\u{1b}[0m"
}

#[doc(hidden)]
pub struct DownloadedAsset {
    pub path: PathBuf,
    pub checksum: String,
}

async fn download_asset(
    client: &Client,
    asset: &ReleaseAsset,
    timeout: Duration,
) -> Result<DownloadedAsset, AutoUpdateError> {
    let mut response = client
        .get(&asset.browser_download_url)
        .timeout(timeout)
        .send()
        .await?
        .error_for_status()?;

    let exe_path = std::env::current_exe()?;
    let temp_path = exe_path
        .parent()
        .ok_or(AutoUpdateError::ExecutablePath)?
        .join(".osu-collect-update.tmp");

    let mut hasher = Sha256::new();
    let mut file = fs::File::create(&temp_path).await?;
    while let Some(chunk) = response.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    set_executable_permissions(&temp_path).await?;

    let checksum: String = hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();

    Ok(DownloadedAsset {
        path: temp_path,
        checksum,
    })
}

async fn fetch_asset_checksum(
    client: &Client,
    asset: &ReleaseAsset,
    assets: &[ReleaseAsset],
) -> Result<String, AutoUpdateError> {
    let checksum_asset = assets
        .iter()
        .find(|candidate| candidate.name == format!("{}.sha256", asset.name))
        .or_else(|| {
            assets.iter().find(|candidate| {
                candidate.name.ends_with(".sha256") && candidate.name.contains(&asset.name)
            })
        })
        .ok_or_else(|| AutoUpdateError::ChecksumMissing(asset.name.clone()))?;

    let body = client
        .get(&checksum_asset.browser_download_url)
        .timeout(AUTO_UPDATE_TIMEOUT)
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?;

    parse_checksum(&body, &asset.name)
}

fn parse_checksum(body: &str, asset_name: &str) -> Result<String, AutoUpdateError> {
    let checksum = body
        .split_whitespace()
        .find(|part| !part.is_empty())
        .ok_or_else(|| AutoUpdateError::ChecksumFormat(asset_name.to_string()))?
        .to_ascii_lowercase();

    if checksum.len() != 64 || !checksum.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(AutoUpdateError::ChecksumFormat(asset_name.to_string()));
    }

    Ok(checksum)
}

#[doc(hidden)]
pub async fn verify_checksum(
    asset: &DownloadedAsset,
    expected: &str,
) -> Result<(), AutoUpdateError> {
    let actual = asset.checksum.to_ascii_lowercase();
    if actual == expected {
        return Ok(());
    }

    let _ = fs::remove_file(&asset.path).await;
    Err(AutoUpdateError::ChecksumMismatch {
        expected: expected.to_string(),
        actual,
    })
}

async fn apply_update(asset: &DownloadedAsset) -> Result<(), AutoUpdateError> {
    let exe_path = std::env::current_exe()?;
    apply_update_to(asset, &exe_path).await
}

#[doc(hidden)]
pub async fn apply_update_to(
    asset: &DownloadedAsset,
    exe_path: &Path,
) -> Result<(), AutoUpdateError> {
    let rollback_path = exe_path.with_extension("rollback");

    fs::copy(exe_path, &rollback_path).await?;

    match fs::rename(&asset.path, exe_path).await {
        Ok(()) => {
            let _ = fs::remove_file(&rollback_path).await;
            Ok(())
        }
        Err(error) => {
            let _ = fs::remove_file(&asset.path).await;
            if let Err(restore_err) = fs::rename(&rollback_path, exe_path).await {
                Err(AutoUpdateError::RollbackFailed(restore_err))
            } else {
                Err(AutoUpdateError::Io(error))
            }
        }
    }
}

async fn set_executable_permissions(path: &Path) -> Result<(), AutoUpdateError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o755);
        fs::set_permissions(path, perms).await?;
    }

    #[cfg(not(unix))]
    {
        let _ = path;
    }

    Ok(())
}

fn parse_release_version(release: &ReleaseResponse) -> Option<Version> {
    parse_version(&release.tag_name).or_else(|| parse_version(&release.name))
}

fn parse_version(input: &str) -> Option<Version> {
    let trimmed = input.trim_start_matches('v');
    Version::parse(trimmed).ok()
}

#[doc(hidden)]
pub fn target_asset_name() -> Option<&'static str> {
    if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("osu-collect-linux-x64")
    } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("osu-collect-windows-x64.exe")
    } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        Some("osu-collect-macos-arm64")
    } else {
        None
    }
}

#[derive(Debug, Deserialize)]
struct ReleaseResponse {
    pub name: String,
    pub tag_name: String,
    pub assets: Vec<ReleaseAsset>,
}

#[derive(Debug, Deserialize)]
struct ReleaseAsset {
    pub name: String,
    pub browser_download_url: String,
}

#[derive(Debug, Error)]
pub enum AutoUpdateError {
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("failed to parse version: {0}")]
    Version(#[from] semver::Error),
    #[error("unable to locate current executable")]
    ExecutablePath,
    #[error("missing asset for platform: {0}")]
    AssetMissing(String),
    #[error("missing checksum for asset: {0}")]
    ChecksumMissing(String),
    #[error("checksum file malformed for asset: {0}")]
    ChecksumFormat(String),
    #[error("checksum mismatch: expected {expected}, actual {actual}")]
    ChecksumMismatch { expected: String, actual: String },
    #[error("failed during IO: {0}")]
    Io(#[from] std::io::Error),
    #[error("unable to determine release version from tag: {0}")]
    UnparseableVersion(String),
    #[error("failed to restore original binary after update failure: {0}")]
    RollbackFailed(std::io::Error),
}

#[cfg(test)]
#[path = "../tests/unit/auto_update.rs"]
mod tests;
