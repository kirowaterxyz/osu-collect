use super::{
    home::InputField,
    messages::{AppMessage, clear_app_message, set_loading_message},
    next_field, prev_field,
};
use crate::{
    config::{
        Config, DisplayConfig, DownloadConfig, LogFormat, LogLevel, LoggingConfig, MirrorConfig,
        RetryFailedOnDownload, ThemeMode,
        constants::{
            ARCHIVE_VALIDATIONS, LOG_FORMATS, LOG_LEVELS, RETRY_FAILED_ON_DOWNLOAD_MODES,
            THEME_MODES, default_threads,
        },
    },
    download::ArchiveValidation,
    utils::expand_tilde,
};

/// A single changed field in the config diff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigDiffEntry {
    /// Display label for the field (matches what appears in the config tab).
    pub label: &'static str,
    /// Current on-disk value as a display string.
    pub old_value: String,
    /// Pending form value as a display string.
    pub new_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthLoginState {
    LoggedOut,
    InProgress(String),
    LoggedIn,
}

/// Action the auth chip's enter key triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipAction {
    Login,
    Cancel,
    Logout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    AuthChip,
    Theme,
    MirrorNerinyan,
    MirrorOsuDirect,
    MirrorSayobot,
    MirrorNekoha,
    MirrorCustomUrl,
    DownloadThreads,
    DownloadNoVideo,
    DownloadArchiveValidation,
    RetryFailedOnDownload,
    LoggingEnabled,
    LoggingLevel,
    LoggingFormat,
    LoggingDirectory,
}

const ALL_CONFIG_FIELDS: &[ConfigField] = &[
    ConfigField::AuthChip,
    ConfigField::Theme,
    ConfigField::DownloadThreads,
    ConfigField::DownloadNoVideo,
    ConfigField::DownloadArchiveValidation,
    ConfigField::RetryFailedOnDownload,
    ConfigField::MirrorOsuDirect,
    ConfigField::MirrorNerinyan,
    ConfigField::MirrorSayobot,
    ConfigField::MirrorNekoha,
    ConfigField::MirrorCustomUrl,
    ConfigField::LoggingEnabled,
    ConfigField::LoggingLevel,
    ConfigField::LoggingFormat,
    ConfigField::LoggingDirectory,
];

impl ConfigField {
    pub fn is_text_input(self) -> bool {
        matches!(
            self,
            ConfigField::MirrorCustomUrl | ConfigField::LoggingDirectory
        )
    }

    pub fn is_stepper(self) -> bool {
        self == ConfigField::DownloadThreads
    }
}

pub struct ConfigTab {
    pub nerinyan: bool,
    pub osu_direct: bool,
    pub sayobot: bool,
    pub nekoha: bool,
    pub custom_mirror: InputField,
    pub login_state: AuthLoginState,
    pub threads: InputField,
    pub no_video: bool,
    pub archive_validation: ArchiveValidation,
    pub retry_failed_on_download: RetryFailedOnDownload,
    pub logging_enabled: bool,
    pub logging_level: LogLevel,
    pub logging_format: LogFormat,
    pub logging_dir: InputField,
    pub theme: ThemeMode,
    pub focus: ConfigField,
    pub message: Option<AppMessage>,
    pub default_threads: u8,
    /// Snapshot of the config as it was when the tab was loaded. Used to
    /// compute the pending-changes diff before saving.
    pub loaded_config: Config,
}

impl ConfigTab {
    pub fn new(config: &Config) -> Self {
        let auth_loaded = crate::auth::load().is_some();
        Self {
            nerinyan: config.mirror.nerinyan,
            osu_direct: config.mirror.osu_direct,
            sayobot: config.mirror.sayobot,
            nekoha: config.mirror.nekoha,
            custom_mirror: custom_mirror_field(&config.mirror),
            login_state: login_state(auth_loaded),
            threads: threads_field(&config.download),
            no_video: config.download.no_video,
            archive_validation: config.download.archive_validation,
            retry_failed_on_download: config.download.retry_failed_on_download,
            logging_enabled: config.logging.enabled,
            logging_level: config.logging.level,
            logging_format: config.logging.format,
            logging_dir: logging_dir_field(&config.logging),
            theme: config.display.theme,
            focus: ConfigField::AuthChip,
            message: None,
            default_threads: default_threads(),
            loaded_config: config.clone(),
        }
    }

    pub fn next_field(&mut self) {
        self.focus = next_field(ALL_CONFIG_FIELDS, self.focus);
    }

    pub fn prev_field(&mut self) {
        self.focus = prev_field(ALL_CONFIG_FIELDS, self.focus);
    }

    /// Increment the thread count by one, capped at `default_threads`.
    pub fn step_up(&mut self) {
        self.step(1);
    }

    /// Decrement the thread count by one, floored at 1.
    pub fn step_down(&mut self) {
        self.step(-1);
    }

    fn step(&mut self, delta: i16) {
        let current = self.resolved_threads() as i16;
        let max = self.default_threads as i16;
        let next = (current + delta).clamp(1, max) as u8;
        self.threads.set_value(next.to_string());
    }

    pub fn handle_char(&mut self, ch: char) {
        clear_app_message(&mut self.message);
        if let Some(field) = self.focused_input_mut() {
            field.insert_char(ch);
        }
    }

    pub fn backspace(&mut self) {
        clear_app_message(&mut self.message);
        if let Some(field) = self.focused_input_mut() {
            field.delete_before_caret();
        }
    }

    /// Delete the char at the caret in the focused text field (`Delete` key).
    pub fn delete_forward(&mut self) {
        clear_app_message(&mut self.message);
        if let Some(field) = self.focused_input_mut() {
            field.delete_at_caret();
        }
    }

    /// Delete the word left of the caret in the focused text field
    /// (alt/ctrl+backspace).
    pub fn backspace_word(&mut self) {
        clear_app_message(&mut self.message);
        if let Some(field) = self.focused_input_mut() {
            field.delete_word_before_caret();
        }
    }

    /// Move the caret in the focused text field. No-op on non-text fields.
    pub fn caret_left(&mut self) {
        if let Some(field) = self.focused_input_mut() {
            field.caret_left();
        }
    }

    pub fn caret_right(&mut self) {
        if let Some(field) = self.focused_input_mut() {
            field.caret_right();
        }
    }

    pub fn caret_home(&mut self) {
        if let Some(field) = self.focused_input_mut() {
            field.caret_home();
        }
    }

    pub fn caret_end(&mut self) {
        if let Some(field) = self.focused_input_mut() {
            field.caret_end();
        }
    }

    /// The focused text input, or `None` for non-text fields. Used by the
    /// renderer to place the caret.
    pub fn focused_input(&self) -> Option<&InputField> {
        match self.focus {
            ConfigField::MirrorCustomUrl => Some(&self.custom_mirror),
            ConfigField::LoggingDirectory => Some(&self.logging_dir),
            _ => None,
        }
    }

    fn focused_input_mut(&mut self) -> Option<&mut InputField> {
        match self.focus {
            ConfigField::MirrorCustomUrl => Some(&mut self.custom_mirror),
            ConfigField::LoggingDirectory => Some(&mut self.logging_dir),
            _ => None,
        }
    }

    pub fn toggle_current(&mut self) {
        clear_app_message(&mut self.message);
        match self.focus {
            ConfigField::Theme => self.cycle_theme(),
            ConfigField::MirrorNerinyan => self.nerinyan = !self.nerinyan,
            ConfigField::MirrorOsuDirect => self.osu_direct = !self.osu_direct,
            ConfigField::MirrorSayobot => self.sayobot = !self.sayobot,
            ConfigField::MirrorNekoha => self.nekoha = !self.nekoha,
            ConfigField::DownloadNoVideo => self.no_video = !self.no_video,
            ConfigField::DownloadArchiveValidation => self.cycle_archive_validation(),
            ConfigField::RetryFailedOnDownload => self.cycle_retry_failed_on_download(),
            ConfigField::LoggingEnabled => self.logging_enabled = !self.logging_enabled,
            ConfigField::LoggingLevel => self.cycle_logging_level(),
            ConfigField::LoggingFormat => self.cycle_logging_format(),
            ConfigField::AuthChip
            | ConfigField::MirrorCustomUrl
            | ConfigField::DownloadThreads
            | ConfigField::LoggingDirectory => {}
        }
    }

    pub fn cycle_theme(&mut self) {
        self.theme = next_value(THEME_MODES, self.theme);
    }

    pub fn cycle_logging_level(&mut self) {
        self.logging_level = next_value(LOG_LEVELS, self.logging_level);
    }

    pub fn cycle_logging_format(&mut self) {
        self.logging_format = next_value(LOG_FORMATS, self.logging_format);
    }

    pub fn cycle_archive_validation(&mut self) {
        self.archive_validation = next_value(ARCHIVE_VALIDATIONS, self.archive_validation);
    }

    pub fn cycle_retry_failed_on_download(&mut self) {
        self.retry_failed_on_download = next_value(
            RETRY_FAILED_ON_DOWNLOAD_MODES,
            self.retry_failed_on_download,
        );
    }

    /// Returns `true` if the pending form values differ from the loaded config.
    pub fn has_pending_changes(&self, pending: &Config) -> bool {
        !self.diff_entries(pending).is_empty()
    }

    /// Returns one [`ConfigDiffEntry`] per field that differs between the
    /// loaded config and `pending`. Fields that are identical are omitted.
    pub fn diff_entries(&self, pending: &Config) -> Vec<ConfigDiffEntry> {
        let loaded = &self.loaded_config;
        let mut entries = Vec::new();

        macro_rules! diff {
            ($label:expr, $old:expr, $new:expr) => {
                let old_s = $old.to_string();
                let new_s = $new.to_string();
                if old_s != new_s {
                    entries.push(ConfigDiffEntry {
                        label: $label,
                        old_value: old_s,
                        new_value: new_s,
                    });
                }
            };
        }

        diff!(
            "theme",
            theme_label(loaded.display.theme),
            theme_label(pending.display.theme)
        );
        diff!(
            "threads",
            loaded
                .download
                .concurrent
                .map(|v| v.to_string())
                .unwrap_or_else(|| self.default_threads.to_string()),
            pending
                .download
                .concurrent
                .map(|v| v.to_string())
                .unwrap_or_else(|| self.default_threads.to_string())
        );
        diff!(
            "no video",
            bool_label(loaded.download.no_video),
            bool_label(pending.download.no_video)
        );
        diff!(
            "verify integrity",
            archive_label(loaded.download.archive_validation),
            archive_label(pending.download.archive_validation)
        );
        diff!(
            "retry failed",
            retry_label(loaded.download.retry_failed_on_download),
            retry_label(pending.download.retry_failed_on_download)
        );
        diff!(
            "mirror: osu!direct",
            bool_label(loaded.mirror.osu_direct),
            bool_label(pending.mirror.osu_direct)
        );
        diff!(
            "mirror: nerinyan",
            bool_label(loaded.mirror.nerinyan),
            bool_label(pending.mirror.nerinyan)
        );
        diff!(
            "mirror: sayobot",
            bool_label(loaded.mirror.sayobot),
            bool_label(pending.mirror.sayobot)
        );
        diff!(
            "mirror: nekoha",
            bool_label(loaded.mirror.nekoha),
            bool_label(pending.mirror.nekoha)
        );
        diff!(
            "custom mirror",
            loaded.mirror.url.as_deref().unwrap_or(""),
            pending.mirror.url.as_deref().unwrap_or("")
        );
        diff!(
            "logging",
            bool_label(loaded.logging.enabled),
            bool_label(pending.logging.enabled)
        );
        diff!(
            "log level",
            log_level_label(loaded.logging.level),
            log_level_label(pending.logging.level)
        );
        diff!(
            "log format",
            log_format_label(loaded.logging.format),
            log_format_label(pending.logging.format)
        );
        diff!(
            "log directory",
            loaded.logging.file_dir.as_deref().unwrap_or(""),
            pending.logging.file_dir.as_deref().unwrap_or("")
        );

        entries
    }

    pub fn build_config(&self) -> Result<Config, String> {
        let concurrent = self.parse_concurrent()?;
        let mirror = MirrorConfig {
            nerinyan: self.nerinyan,
            osu_direct: self.osu_direct,
            sayobot: self.sayobot,
            nekoha: self.nekoha,
            url: self
                .trimmed_custom_mirror()
                .map(|value| value.into_boxed_str()),
        };

        let download = DownloadConfig {
            concurrent,
            no_video: self.no_video,
            archive_validation: self.archive_validation,
            retry_failed_on_download: self.retry_failed_on_download,
        };

        let logging = LoggingConfig {
            enabled: self.logging_enabled,
            level: self.logging_level,
            format: self.logging_format,
            file_dir: self
                .trimmed_logging_dir()
                .map(|value| value.into_boxed_str()),
        };

        Ok(Config {
            mirror,
            download,
            logging,
            display: DisplayConfig { theme: self.theme },
        })
    }

    pub fn set_loading(&mut self, message: impl Into<String>) {
        let text: String = message.into();
        self.login_state = AuthLoginState::InProgress(text.clone());
        set_loading_message(&mut self.message, text);
    }

    pub fn set_login_in_progress(&mut self) {
        self.login_state = AuthLoginState::InProgress(String::new());
    }

    pub fn set_login_complete(&mut self) {
        self.login_state = AuthLoginState::LoggedIn;
    }

    pub fn set_login_failed(&mut self) {
        self.login_state = AuthLoginState::LoggedOut;
    }

    pub fn set_logged_out(&mut self) {
        self.login_state = AuthLoginState::LoggedOut;
    }

    /// Returns the action the chip's enter key should trigger given the current `login_state`.
    pub fn chip_action(&self) -> ChipAction {
        match &self.login_state {
            AuthLoginState::LoggedOut => ChipAction::Login,
            AuthLoginState::InProgress(_) => ChipAction::Cancel,
            AuthLoginState::LoggedIn => ChipAction::Logout,
        }
    }

    pub fn resolved_threads(&self) -> u8 {
        if self.threads.value.trim().is_empty() {
            self.default_threads
        } else {
            self.threads
                .value
                .trim()
                .parse::<u8>()
                .unwrap_or(self.default_threads)
        }
    }

    fn parse_concurrent(&self) -> Result<Option<u8>, String> {
        let trimmed = self.threads.value.trim();
        if trimmed.is_empty() {
            return Ok(None);
        }

        let value = trimmed
            .parse::<u8>()
            .map_err(|_| "Thread count must be a valid number between 1 and 50".to_string())?;
        if value == 0 || value > 50 {
            return Err("Thread count must be between 1 and 50".to_string());
        }

        Ok(Some(value))
    }

    fn trimmed_custom_mirror(&self) -> Option<String> {
        let trimmed = self.custom_mirror.value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    }

    fn trimmed_logging_dir(&self) -> Option<String> {
        let trimmed = self.logging_dir.value.trim();
        if trimmed.is_empty() {
            None
        } else {
            // Expand `~` at save time so the stored path is always absolute.
            Some(expand_tilde(trimmed))
        }
    }
}

fn custom_mirror_field(mirror: &MirrorConfig) -> InputField {
    InputField::new(
        "Custom mirror URL",
        mirror.custom_template().unwrap_or(""),
        "https://example.com/d/{id}",
    )
}

fn login_state(auth_loaded: bool) -> AuthLoginState {
    if auth_loaded {
        AuthLoginState::LoggedIn
    } else {
        AuthLoginState::LoggedOut
    }
}

fn threads_field(download: &DownloadConfig) -> InputField {
    InputField::new(
        "Default thread count",
        download
            .concurrent
            .map(|value| value.to_string())
            .unwrap_or_default(),
        default_threads().to_string(),
    )
}

fn logging_dir_field(logging: &LoggingConfig) -> InputField {
    InputField::new(
        "Logs directory",
        logging.file_dir.as_deref().unwrap_or(""),
        "~/.local/share/osu-collect/logs",
    )
}

fn bool_label(value: bool) -> &'static str {
    if value { "on" } else { "off" }
}

fn theme_label(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Auto => "auto",
        ThemeMode::Default => "truecolor",
        ThemeMode::Sixteen => "16-color",
        ThemeMode::ColorblindSafe => "colorblind-safe",
    }
}

fn archive_label(mode: ArchiveValidation) -> &'static str {
    match mode {
        ArchiveValidation::Off => "off",
        ArchiveValidation::Magic => "basic",
        ArchiveValidation::Eocd => "strict",
    }
}

fn retry_label(mode: RetryFailedOnDownload) -> &'static str {
    match mode {
        RetryFailedOnDownload::Ask => "ask",
        RetryFailedOnDownload::Yes => "yes",
        RetryFailedOnDownload::No => "no",
    }
}

fn log_level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    }
}

fn log_format_label(format: LogFormat) -> &'static str {
    match format {
        LogFormat::Compact => "compact",
        LogFormat::Pretty => "pretty",
    }
}

fn next_value<T: Copy + PartialEq, const N: usize>(values: [T; N], current: T) -> T {
    values
        .iter()
        .position(|&value| value == current)
        .map(|idx| values[(idx + 1) % values.len()])
        .unwrap_or(values[0])
}

#[cfg(test)]
#[path = "../../tests/unit/app_config.rs"]
mod tests;

#[cfg(test)]
#[path = "../../tests/unit/config_save_modal.rs"]
mod tests_save_modal;
