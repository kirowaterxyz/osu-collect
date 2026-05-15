use super::{home::InputField, messages::AppMessage};
use crate::config::{
    Config, DownloadConfig, LogFormat, LogLevel, LoggingConfig, MirrorConfig,
    constants::{LOG_FORMATS, LOG_LEVELS, default_threads},
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
    MirrorCatboyCentral,
    MirrorCatboyUs,
    MirrorCatboyAsia,
    MirrorOsuDirect,
    MirrorSayobot,
    MirrorNekoha,
    MirrorCustomUrl,
    LoginEntry,
    LogoutEntry,
    DownloadSkipExisting,
    DownloadThreads,
    DownloadNoVideo,
    DownloadVerifyZipEocd,
    LoggingEnabled,
    LoggingLevel,
    LoggingFormat,
    LoggingDirectory,
}

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
    pub catboy_central: bool,
    pub catboy_us: bool,
    pub catboy_asia: bool,
    pub osu_direct: bool,
    pub sayobot: bool,
    pub nekoha: bool,
    pub custom_mirror: InputField,
    pub auth_loaded: bool,
    pub login_state: AuthLoginState,
    pub skip_existing: bool,
    pub threads: InputField,
    pub no_video: bool,
    pub verify_zip_eocd: bool,
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
            catboy_central: config.mirror.catboy_central,
            catboy_us: config.mirror.catboy_us,
            catboy_asia: config.mirror.catboy_asia,
            osu_direct: config.mirror.osu_direct,
            sayobot: config.mirror.sayobot,
            nekoha: config.mirror.nekoha,
            custom_mirror: custom_mirror_field(&config.mirror),
            auth_loaded,
            login_state: login_state(auth_loaded),
            skip_existing: config.download.skip_existing,
            threads: threads_field(&config.download),
            no_video: config.download.no_video,
            verify_zip_eocd: config.download.verify_zip_eocd,
            logging_enabled: config.logging.enabled,
            logging_level: config.logging.level,
            logging_format: config.logging.format,
            logging_dir: logging_dir_field(&config.logging),
            focus: ConfigField::LoginEntry,
            message: None,
        }
    }

    pub fn next_field(&mut self) {
        self.focus = match self.focus {
            ConfigField::LoginEntry if self.logout_selectable() => ConfigField::LogoutEntry,
            ConfigField::LoginEntry => ConfigField::DownloadThreads,
            ConfigField::LogoutEntry => ConfigField::DownloadThreads,
            ConfigField::DownloadThreads => ConfigField::DownloadSkipExisting,
            ConfigField::DownloadSkipExisting => ConfigField::DownloadNoVideo,
            ConfigField::DownloadNoVideo => ConfigField::DownloadVerifyZipEocd,
            ConfigField::DownloadVerifyZipEocd => ConfigField::MirrorOsuDirect,
            ConfigField::MirrorOsuDirect => ConfigField::MirrorNerinyan,
            ConfigField::MirrorNerinyan => ConfigField::MirrorSayobot,
            ConfigField::MirrorSayobot => ConfigField::MirrorNekoha,
            ConfigField::MirrorNekoha => ConfigField::MirrorCatboyCentral,
            ConfigField::MirrorCatboyCentral => ConfigField::MirrorCatboyUs,
            ConfigField::MirrorCatboyUs => ConfigField::MirrorCatboyAsia,
            ConfigField::MirrorCatboyAsia => ConfigField::MirrorCustomUrl,
            ConfigField::MirrorCustomUrl => ConfigField::LoggingEnabled,
            ConfigField::LoggingEnabled => ConfigField::LoggingLevel,
            ConfigField::LoggingLevel => ConfigField::LoggingFormat,
            ConfigField::LoggingFormat => ConfigField::LoggingDirectory,
            ConfigField::LoggingDirectory => ConfigField::LoginEntry,
        };
    }

    pub fn prev_field(&mut self) {
        self.focus = match self.focus {
            ConfigField::LoginEntry => ConfigField::LoggingDirectory,
            ConfigField::LogoutEntry => ConfigField::LoginEntry,
            ConfigField::DownloadThreads if self.logout_selectable() => ConfigField::LogoutEntry,
            ConfigField::DownloadThreads => ConfigField::LoginEntry,
            ConfigField::DownloadSkipExisting => ConfigField::DownloadThreads,
            ConfigField::DownloadNoVideo => ConfigField::DownloadSkipExisting,
            ConfigField::DownloadVerifyZipEocd => ConfigField::DownloadNoVideo,
            ConfigField::MirrorOsuDirect => ConfigField::DownloadVerifyZipEocd,
            ConfigField::MirrorNerinyan => ConfigField::MirrorOsuDirect,
            ConfigField::MirrorSayobot => ConfigField::MirrorNerinyan,
            ConfigField::MirrorNekoha => ConfigField::MirrorSayobot,
            ConfigField::MirrorCatboyCentral => ConfigField::MirrorNekoha,
            ConfigField::MirrorCatboyUs => ConfigField::MirrorCatboyCentral,
            ConfigField::MirrorCatboyAsia => ConfigField::MirrorCatboyUs,
            ConfigField::MirrorCustomUrl => ConfigField::MirrorCatboyAsia,
            ConfigField::LoggingEnabled => ConfigField::MirrorCustomUrl,
            ConfigField::LoggingLevel => ConfigField::LoggingEnabled,
            ConfigField::LoggingFormat => ConfigField::LoggingLevel,
            ConfigField::LoggingDirectory => ConfigField::LoggingFormat,
        };
    }

    pub fn handle_char(&mut self, ch: char) {
        self.clear_message();
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
        self.clear_message();
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
        self.clear_message();
        match self.focus {
            ConfigField::MirrorNerinyan => self.nerinyan = !self.nerinyan,
            ConfigField::MirrorCatboyCentral => self.catboy_central = !self.catboy_central,
            ConfigField::MirrorCatboyUs => self.catboy_us = !self.catboy_us,
            ConfigField::MirrorCatboyAsia => self.catboy_asia = !self.catboy_asia,
            ConfigField::MirrorOsuDirect => self.osu_direct = !self.osu_direct,
            ConfigField::MirrorSayobot => self.sayobot = !self.sayobot,
            ConfigField::MirrorNekoha => self.nekoha = !self.nekoha,
            ConfigField::DownloadSkipExisting => self.skip_existing = !self.skip_existing,
            ConfigField::DownloadNoVideo => self.no_video = !self.no_video,
            ConfigField::DownloadVerifyZipEocd => {
                self.verify_zip_eocd = !self.verify_zip_eocd;
            }
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

    pub fn build_config(&self) -> Result<Config, String> {
        let concurrent = self.parse_concurrent()?;
        let mirror = MirrorConfig {
            nerinyan: self.nerinyan,
            catboy_central: self.catboy_central,
            catboy_us: self.catboy_us,
            catboy_asia: self.catboy_asia,
            osu_direct: self.osu_direct,
            sayobot: self.sayobot,
            nekoha: self.nekoha,
            url: self
                .trimmed_custom_mirror()
                .map(|value| value.into_boxed_str()),
        };

        let download = DownloadConfig {
            skip_existing: self.skip_existing,
            concurrent,
            no_video: self.no_video,
            verify_zip_eocd: self.verify_zip_eocd,
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

    pub fn set_error(&mut self, message: impl Into<String>) {
        self.message = Some(AppMessage::error(message));
    }

    pub fn set_info(&mut self, message: impl Into<String>) {
        self.message = Some(AppMessage::info(message));
    }

    pub fn set_loading(&mut self, message: impl Into<String>) {
        let text = message.into();
        self.login_state = AuthLoginState::InProgress(text.clone());
        self.message = Some(AppMessage::loading(text));
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

    fn evacuate_logout_focus(&mut self) {
        if self.focus == ConfigField::LogoutEntry && !self.logout_selectable() {
            self.focus = ConfigField::LoginEntry;
        }
    }

    pub fn clear_message(&mut self) {
        self.message = None;
    }

    pub fn clear_expired_message(&mut self) {
        if self.message.as_ref().is_some_and(AppMessage::is_expired) {
            self.message = None;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    fn tab_logged_out() -> ConfigTab {
        let mut tab = ConfigTab::new(&Config::default());
        tab.auth_loaded = false;
        tab.login_state = AuthLoginState::LoggedOut;
        tab
    }

    fn tab_logged_in() -> ConfigTab {
        let mut tab = ConfigTab::new(&Config::default());
        tab.auth_loaded = true;
        tab.login_state = AuthLoginState::LoggedIn;
        tab
    }

    #[test]
    fn login_state_initial_logged_out() {
        let tab = tab_logged_out();
        assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
        assert!(!tab.auth_loaded);
    }

    #[test]
    fn login_state_initial_logged_in() {
        let tab = tab_logged_in();
        assert_eq!(tab.login_state, AuthLoginState::LoggedIn);
        assert!(tab.auth_loaded);
    }

    #[test]
    fn login_flow_marks_in_progress_without_message() {
        let mut tab = tab_logged_out();
        tab.set_login_in_progress();
        assert_eq!(tab.login_state, AuthLoginState::InProgress(String::new()));
        assert!(tab.message.is_none());
        assert!(!tab.auth_loaded);
    }

    #[test]
    fn login_flow_success() {
        let mut tab = tab_logged_out();
        tab.set_login_in_progress();
        tab.set_login_complete();
        assert_eq!(tab.login_state, AuthLoginState::LoggedIn);
        assert!(tab.auth_loaded);
    }

    #[test]
    fn login_flow_error_returns_to_logged_out() {
        let mut tab = tab_logged_out();
        tab.set_login_in_progress();
        tab.set_login_failed();
        assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
        assert!(!tab.auth_loaded);
    }

    #[test]
    fn cancel_login_returns_to_logged_out_with_info_message() {
        use crate::app::messages::MessageKind;

        let mut tab = tab_logged_out();
        tab.set_login_in_progress();
        assert!(matches!(tab.login_state, AuthLoginState::InProgress(_)));

        tab.set_login_failed();
        tab.set_info("login cancelled");

        assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
        let msg = tab.message.as_ref().expect("info message preserved");
        assert_eq!(msg.kind, MessageKind::Info);
        assert_eq!(msg.text, "login cancelled");
    }

    #[test]
    fn logout_clears_state() {
        let mut tab = tab_logged_in();
        tab.set_logged_out();
        assert_eq!(tab.login_state, AuthLoginState::LoggedOut);
        assert!(!tab.auth_loaded);
    }

    #[test]
    fn logout_loading_message_does_not_expire() {
        let mut tab = tab_logged_in();
        tab.set_loading("logging out...");
        let msg = tab.message.as_ref().unwrap();
        assert!(!msg.is_expired());
    }

    #[test]
    fn next_field_cycles_through_login_entries() {
        let mut tab = tab_logged_in();
        tab.focus = ConfigField::LoggingDirectory;
        tab.next_field();
        assert_eq!(tab.focus, ConfigField::LoginEntry);
        tab.next_field();
        assert_eq!(tab.focus, ConfigField::LogoutEntry);
        tab.next_field();
        assert_eq!(tab.focus, ConfigField::DownloadThreads);
    }

    #[test]
    fn prev_field_cycles_through_login_entries() {
        let mut tab = tab_logged_in();
        tab.focus = ConfigField::DownloadThreads;
        tab.prev_field();
        assert_eq!(tab.focus, ConfigField::LogoutEntry);
        tab.prev_field();
        assert_eq!(tab.focus, ConfigField::LoginEntry);
        tab.prev_field();
        assert_eq!(tab.focus, ConfigField::LoggingDirectory);
    }

    #[test]
    fn next_field_skips_logout_when_logged_out() {
        let mut tab = tab_logged_out();
        tab.focus = ConfigField::LoginEntry;
        tab.next_field();
        assert_eq!(tab.focus, ConfigField::DownloadThreads);
    }

    #[test]
    fn prev_field_skips_logout_when_logged_out() {
        let mut tab = tab_logged_out();
        tab.focus = ConfigField::DownloadThreads;
        tab.prev_field();
        assert_eq!(tab.focus, ConfigField::LoginEntry);
    }

    #[test]
    fn logout_evacuates_focus_when_logging_out() {
        let mut tab = tab_logged_in();
        tab.focus = ConfigField::LogoutEntry;
        tab.set_logged_out();
        assert_eq!(tab.focus, ConfigField::LoginEntry);
    }

    #[test]
    fn all_fields_form_complete_cycle() {
        let mut tab = tab_logged_in();
        let start = tab.focus;
        let total = 18;
        for _ in 0..total {
            tab.next_field();
        }
        assert_eq!(tab.focus, start, "next_field must complete a full cycle");
    }
}
