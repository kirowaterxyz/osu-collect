use super::{
    home::InputField,
    messages::{AppMessage, clear_app_message, set_loading_message},
    next_field, prev_field,
};
use crate::{
    config::{
        Config, DownloadConfig, LogFormat, LogLevel, LoggingConfig, MirrorConfig,
        constants::{ARCHIVE_VALIDATIONS, LOG_FORMATS, LOG_LEVELS, default_threads},
    },
    download::ArchiveValidation,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthLoginState {
    LoggedOut,
    InProgress(String),
    LoggedIn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigField {
    MirrorNerinyan,
    MirrorOsuDirect,
    MirrorSayobot,
    MirrorNekoha,
    MirrorCustomUrl,
    LoginEntry,
    LogoutEntry,
    DownloadThreads,
    DownloadNoVideo,
    DownloadArchiveValidation,
    LoggingEnabled,
    LoggingLevel,
    LoggingFormat,
    LoggingDirectory,
}

const LOGGED_IN_CONFIG_FIELDS: &[ConfigField] = &[
    ConfigField::LoginEntry,
    ConfigField::LogoutEntry,
    ConfigField::DownloadThreads,
    ConfigField::DownloadNoVideo,
    ConfigField::DownloadArchiveValidation,
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

const LOGGED_OUT_CONFIG_FIELDS: &[ConfigField] = &[
    ConfigField::LoginEntry,
    ConfigField::DownloadThreads,
    ConfigField::DownloadNoVideo,
    ConfigField::DownloadArchiveValidation,
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
            ConfigField::MirrorCustomUrl
                | ConfigField::DownloadThreads
                | ConfigField::LoggingDirectory
        )
    }
}

pub struct ConfigTab {
    pub nerinyan: bool,
    pub osu_direct: bool,
    pub sayobot: bool,
    pub nekoha: bool,
    pub custom_mirror: InputField,
    pub auth_loaded: bool,
    pub login_state: AuthLoginState,
    pub threads: InputField,
    pub no_video: bool,
    pub archive_validation: ArchiveValidation,
    pub logging_enabled: bool,
    pub logging_level: LogLevel,
    pub logging_format: LogFormat,
    pub logging_dir: InputField,
    pub focus: ConfigField,
    pub message: Option<AppMessage>,
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
            auth_loaded,
            login_state: login_state(auth_loaded),
            threads: threads_field(&config.download),
            no_video: config.download.no_video,
            archive_validation: config.download.archive_validation,
            logging_enabled: config.logging.enabled,
            logging_level: config.logging.level,
            logging_format: config.logging.format,
            logging_dir: logging_dir_field(&config.logging),
            focus: ConfigField::LoginEntry,
            message: None,
        }
    }

    pub fn next_field(&mut self) {
        let fields = self.fields();
        self.focus = next_field(fields, self.focus);
    }

    pub fn prev_field(&mut self) {
        let fields = self.fields();
        self.focus = prev_field(fields, self.focus);
    }

    pub fn handle_char(&mut self, ch: char) {
        clear_app_message(&mut self.message);
        match self.focus {
            ConfigField::MirrorCustomUrl => self.custom_mirror.value.push(ch),
            ConfigField::DownloadThreads if ch.is_ascii_digit() => {
                self.threads.value.push(ch);
            }
            ConfigField::LoggingDirectory => self.logging_dir.value.push(ch),
            _ => {}
        }
    }

    pub fn backspace(&mut self) {
        clear_app_message(&mut self.message);
        match self.focus {
            ConfigField::MirrorCustomUrl => {
                self.custom_mirror.value.pop();
            }
            ConfigField::DownloadThreads => {
                self.threads.value.pop();
            }
            ConfigField::LoggingDirectory => {
                self.logging_dir.value.pop();
            }
            _ => {}
        }
    }

    pub fn toggle_current(&mut self) {
        clear_app_message(&mut self.message);
        match self.focus {
            ConfigField::MirrorNerinyan => self.nerinyan = !self.nerinyan,
            ConfigField::MirrorOsuDirect => self.osu_direct = !self.osu_direct,
            ConfigField::MirrorSayobot => self.sayobot = !self.sayobot,
            ConfigField::MirrorNekoha => self.nekoha = !self.nekoha,
            ConfigField::DownloadNoVideo => self.no_video = !self.no_video,
            ConfigField::DownloadArchiveValidation => self.cycle_archive_validation(),
            ConfigField::LoggingEnabled => self.logging_enabled = !self.logging_enabled,
            ConfigField::LoggingLevel => self.cycle_logging_level(),
            ConfigField::LoggingFormat => self.cycle_logging_format(),
            ConfigField::MirrorCustomUrl
            | ConfigField::LoginEntry
            | ConfigField::LogoutEntry
            | ConfigField::DownloadThreads
            | ConfigField::LoggingDirectory => {}
        }
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
        })
    }

    pub fn set_loading(&mut self, message: impl Into<String>) {
        let text: String = message.into();
        self.login_state = AuthLoginState::InProgress(text.clone());
        set_loading_message(&mut self.message, text);
        self.evacuate_logout_focus();
    }

    pub fn set_login_in_progress(&mut self) {
        self.login_state = AuthLoginState::InProgress(String::new());
        self.evacuate_logout_focus();
    }

    pub fn set_login_complete(&mut self) {
        self.auth_loaded = true;
        self.login_state = AuthLoginState::LoggedIn;
    }

    pub fn set_login_failed(&mut self) {
        self.auth_loaded = false;
        self.login_state = AuthLoginState::LoggedOut;
        self.evacuate_logout_focus();
    }

    pub fn set_logged_out(&mut self) {
        self.auth_loaded = false;
        self.login_state = AuthLoginState::LoggedOut;
        self.evacuate_logout_focus();
    }

    pub fn logout_selectable(&self) -> bool {
        matches!(self.login_state, AuthLoginState::LoggedIn)
    }

    fn fields(&self) -> &'static [ConfigField] {
        if self.logout_selectable() {
            LOGGED_IN_CONFIG_FIELDS
        } else {
            LOGGED_OUT_CONFIG_FIELDS
        }
    }

    fn evacuate_logout_focus(&mut self) {
        if self.focus == ConfigField::LogoutEntry && !self.logout_selectable() {
            self.focus = ConfigField::LoginEntry;
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
            Some(trimmed.to_string())
        }
    }
}

fn custom_mirror_field(mirror: &MirrorConfig) -> InputField {
    InputField {
        label: "Custom mirror URL",
        value: mirror.custom_template().unwrap_or("").to_string(),
        placeholder: "https://example.com/d/{id}".to_string(),
    }
}

fn login_state(auth_loaded: bool) -> AuthLoginState {
    if auth_loaded {
        AuthLoginState::LoggedIn
    } else {
        AuthLoginState::LoggedOut
    }
}

fn threads_field(download: &DownloadConfig) -> InputField {
    InputField {
        label: "Default thread count",
        value: download
            .concurrent
            .map(|value| value.to_string())
            .unwrap_or_default(),
        placeholder: default_threads().to_string(),
    }
}

fn logging_dir_field(logging: &LoggingConfig) -> InputField {
    InputField {
        label: "Logs directory",
        value: logging.file_dir.as_deref().unwrap_or("").to_string(),
        placeholder: "~/.local/share/osu-collect/logs".to_string(),
    }
}

fn next_value<T: Copy + PartialEq, const N: usize>(values: [T; N], current: T) -> T {
    values
        .iter()
        .position(|&value| value == current)
        .map(|idx| values[(idx + 1) % values.len()])
        .unwrap_or(values[0])
}
