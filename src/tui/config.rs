use crate::{
    app::{AuthLoginState, ConfigField, ConfigTab},
    config::{LogFormat, LogLevel, RetryFailedOnDownload, ThemeMode},
    download::ArchiveValidation,
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use super::widgets;
use super::{
    HELP_CUSTOM_MIRROR, accent_alt, focused_label, mirror_label, success, text_faint, warning,
};
use osu_downloader::MirrorKind;

const PANEL_TITLE: &str = " CONFIG ";

const TOP_BANNER: &str = "default settings and config options";

const SECTION_DISPLAY: &str = "display";
const SECTION_DOWNLOAD: &str = "download";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_LOGGING: &str = "logging";

const LABEL_THEME: &str = "theme";

const LABEL_SKIP_VIDEOS: &str = "skip videos";
const LABEL_VERIFY_INTEGRITY: &str = "verify .osz integrity";
const LABEL_RETRY_FAILED: &str = "retry failed on download";
const LABEL_LOGGING_ENABLED: &str = "enable logging";
const LABEL_LOGGING_LEVEL: &str = "level";
const LABEL_LOGGING_FORMAT: &str = "format";

const LOGIN_HEADER: &str = "OSU! LOGIN";
const LOGIN_UNAVAILABLE: &str = "login unavailable (no credentials in build)";
const LOGIN_LOG_IN: &str = "log in";
const LOGIN_LOGGING_IN: &str = "logging in...";
const LOGIN_CANCEL_HINT: &str = " (cancel?)";
const LOGIN_RE_LOGIN: &str = "re-login";
const LOGIN_LOG_OUT: &str = "log out";
const STATUS_LOGGED_OUT: &str = "logged out";
const STATUS_LOGGED_IN: &str = "logged in";

const THEME_MODE_LABELS: &[&str] = &["auto", "default", "16-color", "colorblind-safe"];

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
const LOG_FORMATS: &[&str] = &["compact", "pretty"];
const ARCHIVE_VALIDATION_LABELS: &[&str] = &["off", "basic", "strict"];
const RETRY_FAILED_LABELS: &[&str] = &["ask", "yes", "no"];

const HELP_VERIFY_STRICT: &str = "strict mode may reject beatmaps that osu! would still accept";
const HELP_VERIFY_INTEGRITY: &str =
    "off skips checks; basic verifies headers; strict also checks eocd footer";
const HELP_RETRY_FAILED: &str =
    "ask: prompt before each download · yes: always retry · no: never retry";

pub fn render(frame: &mut Frame, area: Rect, form: &ConfigTab) {
    let focus = form.focus;
    let mut items = widgets::FormItems::new(focus);

    items.push(widgets::disclosure_row(TOP_BANNER, "", false, false));
    items.push(widgets::spacer());

    items.push(login_section_header(&form.login_state));
    items.push_focusable(ConfigField::LoginEntry, login_entry_item(form));
    items.push_focusable(ConfigField::LogoutEntry, logout_entry_item(form));
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_DISPLAY));
    items.push_focusable(
        ConfigField::Theme,
        widgets::cycle_item(
            LABEL_THEME,
            THEME_MODE_LABELS,
            theme_mode_label(form.theme),
            focus == ConfigField::Theme,
        ),
    );
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_DOWNLOAD));
    items.push_focusable(
        ConfigField::DownloadThreads,
        widgets::stepper_item(
            form.threads.label,
            form.resolved_threads(),
            form.default_threads,
            focus == ConfigField::DownloadThreads,
        ),
    );
    items.push_focusable(
        ConfigField::DownloadNoVideo,
        widgets::row_item(
            LABEL_SKIP_VIDEOS,
            Some(bool_label(form.no_video)),
            form.no_video,
            focus == ConfigField::DownloadNoVideo,
        ),
    );
    items.push_focusable(
        ConfigField::DownloadArchiveValidation,
        widgets::cycle_item(
            LABEL_VERIFY_INTEGRITY,
            ARCHIVE_VALIDATION_LABELS,
            archive_validation_label(form.archive_validation),
            focus == ConfigField::DownloadArchiveValidation,
        ),
    );
    if focus == ConfigField::DownloadArchiveValidation {
        let help = if form.archive_validation == ArchiveValidation::Eocd {
            HELP_VERIFY_STRICT
        } else {
            HELP_VERIFY_INTEGRITY
        };
        items.push(widgets::help_item(help));
    }
    items.push_focusable(
        ConfigField::RetryFailedOnDownload,
        widgets::cycle_item(
            LABEL_RETRY_FAILED,
            RETRY_FAILED_LABELS,
            retry_failed_label(form.retry_failed_on_download),
            focus == ConfigField::RetryFailedOnDownload,
        ),
    );
    if focus == ConfigField::RetryFailedOnDownload {
        items.push(widgets::help_item(HELP_RETRY_FAILED));
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_MIRRORS));
    let mirror_states = [
        (ConfigField::MirrorOsuDirect, form.osu_direct),
        (ConfigField::MirrorNerinyan, form.nerinyan),
        (ConfigField::MirrorSayobot, form.sayobot),
        (ConfigField::MirrorNekoha, form.nekoha),
    ];
    for (kind, (field, on)) in MirrorKind::BUILTINS.iter().zip(mirror_states) {
        items.push_focusable(
            field,
            widgets::row_item(mirror_label(*kind), Some(kind.host()), on, focus == field),
        );
    }
    items.push_focusable(
        ConfigField::MirrorCustomUrl,
        widgets::input_item(&form.custom_mirror, focus == ConfigField::MirrorCustomUrl),
    );
    if focus == ConfigField::MirrorCustomUrl {
        items.push(widgets::help_item(HELP_CUSTOM_MIRROR));
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_LOGGING));
    items.push_focusable(
        ConfigField::LoggingEnabled,
        widgets::row_item(
            LABEL_LOGGING_ENABLED,
            Some(bool_label(form.logging_enabled)),
            form.logging_enabled,
            focus == ConfigField::LoggingEnabled,
        ),
    );
    items.push_focusable(
        ConfigField::LoggingLevel,
        widgets::cycle_item(
            LABEL_LOGGING_LEVEL,
            LOG_LEVELS,
            log_level_label(form.logging_level),
            focus == ConfigField::LoggingLevel,
        ),
    );
    items.push_focusable(
        ConfigField::LoggingFormat,
        widgets::cycle_item(
            LABEL_LOGGING_FORMAT,
            LOG_FORMATS,
            log_format_label(form.logging_format),
            focus == ConfigField::LoggingFormat,
        ),
    );
    items.push_focusable(
        ConfigField::LoggingDirectory,
        widgets::input_item(&form.logging_dir, focus == ConfigField::LoggingDirectory),
    );

    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index);
}

fn login_entry_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::LoginEntry;
    let available = crate::auth::bundled_credentials().is_some();

    let mut spans = vec![widgets::focus_span(focused)];
    if !available {
        spans.push(Span::styled(
            LOGIN_UNAVAILABLE,
            Style::default().fg(text_faint()),
        ));
    } else {
        match &form.login_state {
            AuthLoginState::LoggedOut => {
                spans.push(Span::styled(LOGIN_LOG_IN, focused_label(focused)));
            }
            AuthLoginState::InProgress(_) => {
                spans.push(Span::styled(
                    LOGIN_LOGGING_IN,
                    Style::default().fg(warning()),
                ));
                spans.push(Span::styled(LOGIN_CANCEL_HINT, focused_label(focused)));
            }
            AuthLoginState::LoggedIn => {
                spans.push(Span::styled(LOGIN_RE_LOGIN, focused_label(focused)));
            }
        }
    }

    ListItem::new(Line::from(spans))
}

fn logout_entry_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::LogoutEntry;
    let enabled = matches!(form.login_state, AuthLoginState::LoggedIn);

    let style = if enabled {
        focused_label(focused)
    } else {
        Style::default().fg(text_faint())
    };

    ListItem::new(Line::from(vec![
        widgets::focus_span(focused && enabled),
        Span::styled(LOGIN_LOG_OUT, style),
    ]))
}

fn login_section_header(state: &AuthLoginState) -> ListItem<'static> {
    let status = match state {
        AuthLoginState::LoggedOut => Some((
            STATUS_LOGGED_OUT.to_string(),
            Style::default().fg(text_faint()),
        )),
        AuthLoginState::InProgress(step) if !step.is_empty() => Some((
            step.clone(),
            Style::default()
                .fg(warning())
                .add_modifier(Modifier::ITALIC),
        )),
        AuthLoginState::InProgress(_) => None,
        AuthLoginState::LoggedIn => Some((
            STATUS_LOGGED_IN.to_string(),
            Style::default().fg(success()).add_modifier(Modifier::BOLD),
        )),
    };

    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            LOGIN_HEADER,
            Style::default()
                .fg(accent_alt())
                .add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some((text, style)) = status {
        spans.push(" ".into());
        spans.push(Span::styled(text, style));
    }
    ListItem::new(Line::from(spans))
}

fn bool_label(value: bool) -> &'static str {
    if value { "true" } else { "false" }
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

fn archive_validation_label(mode: ArchiveValidation) -> &'static str {
    match mode {
        ArchiveValidation::Off => "off",
        ArchiveValidation::Magic => "basic",
        ArchiveValidation::Eocd => "strict",
    }
}

fn retry_failed_label(mode: RetryFailedOnDownload) -> &'static str {
    match mode {
        RetryFailedOnDownload::Ask => "ask",
        RetryFailedOnDownload::Yes => "yes",
        RetryFailedOnDownload::No => "no",
    }
}

fn theme_mode_label(mode: ThemeMode) -> &'static str {
    match mode {
        ThemeMode::Auto => "auto",
        ThemeMode::Default => "default",
        ThemeMode::Sixteen => "16-color",
        ThemeMode::ColorblindSafe => "colorblind-safe",
    }
}
