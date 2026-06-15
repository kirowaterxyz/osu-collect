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
    #[serde(default)]
    pub recent: RecentConfig,
}

/// Last-used home-tab inputs, persisted across runs so the collection field and
/// download directory pre-fill with whatever the user downloaded last.
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct RecentConfig {
    /// Last collection URL or ID typed into the home form.
    pub collection: Option<String>,
    /// Last download directory typed into the home form.
    pub directory: Option<String>,
}

/// Theme selection for the TUI.
///
/// The palette defaults to [`ThemeMode::Full`]. When `display.theme` is absent
/// from config entirely (first run, or a config that failed to parse), the full
/// truecolor palette is used — there is no terminal auto-detection. See
/// `tui::theme::apply_theme`.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeMode {
    /// Force the full Catppuccin Mocha truecolor (RGB) palette.
    #[default]
    Full,
    /// Force the xterm-256 compatible palette.
    Compatible,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
#[serde(default)]
pub struct DisplayConfig {
    /// Explicit palette choice. `None` (key absent) selects the full truecolor
    /// palette at startup. Any value the user picks in the config tab pins the
    /// choice from then on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<ThemeMode>,
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
    /// Anonymous beatconnect.io CDN download. On by default.
    #[serde(default = "default_enabled")]
    pub beatconnect: bool,
    /// Hinamizawa cascade. Off by default: it races server-side through the
    /// other mirrors, so enabling it alongside them is redundant.
    #[serde(default)]
    pub hinamizawa: bool,
    /// Official osu! API download. Off by default: needs an interactive
    /// `lazer`-scope login and is rate-limited to 10–20 downloads/hour.
    #[serde(default)]
    pub osu_official: bool,
    #[serde(default)]
    pub url: Option<Box<str>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(default)]
pub struct DownloadConfig {
    pub concurrent: Option<u8>,
    /// Whether beatmap videos are included in downloads. `true` (the default)
    /// uses each mirror's full template; `false` switches to its no-video one.
    pub video: bool,
    pub archive_validation: ArchiveValidation,
    pub retry_failed_on_download: RetryFailedOnDownload,
}

impl Default for DownloadConfig {
    fn default() -> Self {
        Self {
            concurrent: None,
            video: true,
            archive_validation: ArchiveValidation::default(),
            retry_failed_on_download: RetryFailedOnDownload::default(),
        }
    }
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
            beatconnect: true,
            hinamizawa: false,
            osu_official: false,
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
            || self.beatconnect
            || self.hinamizawa
            || self.osu_official
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

            if concurrent > 100 {
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
