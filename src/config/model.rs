use crate::{
    download::ArchiveValidation,
    mirrors,
    utils::{AppError, Result},
};
use serde::{Deserialize, Serialize};
use tracing::warn;

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct Config {
    #[serde(default)]
    pub mirror: MirrorConfig,
    #[serde(default)]
    pub download: DownloadConfig,
    #[serde(default)]
    pub logging: LoggingConfig,
    #[serde(default)]
    pub display: DisplayConfig,
}

/// Theme selection for the TUI.
///
/// `Auto` detects the terminal's color depth at startup and falls back to the
/// 16-color palette when only basic ANSI colors are available.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    /// Auto-detect: truecolor → default palette; 16-color terminal → sixteen palette.
    #[default]
    Auto,
    /// Force the default Catppuccin-style truecolor palette.
    Default,
    /// Force the 16-color ANSI fallback palette.
    Sixteen,
    /// Force the colorblind-safe (Wong/IBM) palette.
    ColorblindSafe,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct DisplayConfig {
    pub theme: ThemeMode,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MirrorConfig {
    #[serde(default = "default_enabled")]
    pub nerinyan: bool,
    #[serde(default = "default_enabled")]
    pub osu_direct: bool,
    #[serde(default = "default_enabled")]
    pub sayobot: bool,
    #[serde(default = "default_enabled")]
    pub nekoha: bool,
    #[serde(default)]
    pub url: Option<Box<str>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct DownloadConfig {
    pub concurrent: Option<u8>,
    pub no_video: bool,
    pub archive_validation: ArchiveValidation,
    pub retry_failed_on_download: RetryFailedOnDownload,
}

/// Policy for retrying beatmaps that failed in a previous run when the user
/// kicks off a new download for the same collection.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum RetryFailedOnDownload {
    /// Prompt the user before each download when failures intersect.
    #[default]
    Ask,
    /// Always retry — include previously failed beatmaps in the download.
    Yes,
    /// Never retry — skip previously failed beatmaps silently.
    No,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct LoggingConfig {
    pub enabled: bool,
    pub level: LogLevel,
    pub format: LogFormat,
    pub file_dir: Option<Box<str>>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LogLevel {
    Error,
    Warn,
    #[default]
    Info,
    Debug,
    Trace,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum LogFormat {
    #[default]
    Compact,
    Pretty,
}

fn default_enabled() -> bool {
    true
}

impl Default for MirrorConfig {
    fn default() -> Self {
        Self {
            nerinyan: true,
            osu_direct: true,
            sayobot: true,
            nekoha: true,
            url: None,
        }
    }
}

impl MirrorConfig {
    pub fn custom_template(&self) -> Option<&str> {
        self.url
            .as_deref()
            .filter(|&value| !value.trim().is_empty())
    }

    fn any_enabled(&self) -> bool {
        self.nerinyan
            || self.osu_direct
            || self.sayobot
            || self.nekoha
            || self.custom_template().is_some()
    }
}

impl DownloadConfig {
    pub fn resolved_concurrent(&self) -> u8 {
        self.concurrent
            .unwrap_or_else(super::constants::default_threads)
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            level: LogLevel::Info,
            format: LogFormat::Compact,
            file_dir: None,
        }
    }
}

impl Config {
    pub fn validate(&self) -> Result<()> {
        if !self.mirror.any_enabled() {
            return Err(AppError::config("Enable at least one mirror"));
        }

        if let Some(custom) = self.mirror.custom_template() {
            mirrors::Mirror::custom(custom).map_err(|e| AppError::config_dynamic(e.to_string()))?;
        }

        if let Some(concurrent) = self.download.concurrent {
            if concurrent == 0 {
                return Err(AppError::config("Thread count must be at least 1"));
            }

            if concurrent > 50 {
                warn!(
                    concurrent,
                    "Thread count is unusually high; consider lowering to avoid rate limiting"
                );
                warn!("Recommended maximum is 20 to avoid rate limiting");
            }
        }

        if self.logging.enabled
            && let Some(dir) = self.logging.file_dir.as_deref()
            && dir.trim().is_empty()
        {
            return Err(AppError::config(
                "logging.file_dir cannot be empty when logging is enabled",
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
#[path = "../../tests/unit/config_theme.rs"]
mod tests;
