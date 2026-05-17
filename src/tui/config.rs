use crate::{
    app::{AuthLoginState, ConfigField, ConfigTab},
    config::{LogFormat, LogLevel},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::ListItem,
};

use super::widgets;
use super::{ACCENT_ALT, HELP_CUSTOM_MIRROR, MIRRORS, SUCCESS, TEXT_FAINT, WARNING, focused_label};

const PANEL_TITLE: &str = "config";

const TOP_BANNER: &str = "default settings and config options";

const SECTION_DOWNLOAD: &str = "download";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_LOGGING: &str = "logging";

const LABEL_SKIP_VIDEOS: &str = "skip videos";
const LABEL_VERIFY_INTEGRITY: &str = "verify .osz integrity";
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

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
const LOG_FORMATS: &[&str] = &["compact", "pretty"];

pub fn render(frame: &mut Frame, area: Rect, form: &ConfigTab) {
    let mut items: Vec<ListItem<'static>> = Vec::new();
    let mut focused_index = 0usize;
    let focus = form.focus;

    let push = |items: &mut Vec<ListItem<'static>>,
                idx: &mut usize,
                field: ConfigField,
                item: ListItem<'static>| {
        if focus == field {
            *idx = items.len();
        }
        items.push(item);
    };

    items.push(widgets::disclosure_row(TOP_BANNER, "", false, false));
    items.push(widgets::spacer());

    items.push(login_section_header(&form.login_state));
    push(
        &mut items,
        &mut focused_index,
        ConfigField::LoginEntry,
        login_entry_item(form),
    );
    push(
        &mut items,
        &mut focused_index,
        ConfigField::LogoutEntry,
        logout_entry_item(form),
    );
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_DOWNLOAD));
    push(
        &mut items,
        &mut focused_index,
        ConfigField::DownloadThreads,
        widgets::input_item(&form.threads, focus == ConfigField::DownloadThreads),
    );
    push(
        &mut items,
        &mut focused_index,
        ConfigField::DownloadNoVideo,
        widgets::row_item(
            LABEL_SKIP_VIDEOS,
            None,
            form.no_video,
            focus == ConfigField::DownloadNoVideo,
        ),
    );
    push(
        &mut items,
        &mut focused_index,
        ConfigField::DownloadVerifyZipEocd,
        widgets::row_item(
            LABEL_VERIFY_INTEGRITY,
            None,
            form.verify_zip_eocd,
            focus == ConfigField::DownloadVerifyZipEocd,
        ),
    );
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_MIRRORS));
    let mirror_states = [
        (ConfigField::MirrorOsuDirect, form.osu_direct),
        (ConfigField::MirrorNerinyan, form.nerinyan),
        (ConfigField::MirrorSayobot, form.sayobot),
        (ConfigField::MirrorNekoha, form.nekoha),
    ];
    for ((label, url), (field, on)) in MIRRORS.iter().zip(mirror_states) {
        push(
            &mut items,
            &mut focused_index,
            field,
            widgets::row_item(label, Some(url), on, focus == field),
        );
    }
    push(
        &mut items,
        &mut focused_index,
        ConfigField::MirrorCustomUrl,
        widgets::input_item(&form.custom_mirror, focus == ConfigField::MirrorCustomUrl),
    );
    items.push(widgets::help_item(HELP_CUSTOM_MIRROR));
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_LOGGING));
    push(
        &mut items,
        &mut focused_index,
        ConfigField::LoggingEnabled,
        widgets::row_item(
            LABEL_LOGGING_ENABLED,
            None,
            form.logging_enabled,
            focus == ConfigField::LoggingEnabled,
        ),
    );
    push(
        &mut items,
        &mut focused_index,
        ConfigField::LoggingLevel,
        widgets::cycle_item(
            LABEL_LOGGING_LEVEL,
            LOG_LEVELS,
            log_level_label(form.logging_level),
            focus == ConfigField::LoggingLevel,
        ),
    );
    push(
        &mut items,
        &mut focused_index,
        ConfigField::LoggingFormat,
        widgets::cycle_item(
            LABEL_LOGGING_FORMAT,
            LOG_FORMATS,
            log_format_label(form.logging_format),
            focus == ConfigField::LoggingFormat,
        ),
    );
    push(
        &mut items,
        &mut focused_index,
        ConfigField::LoggingDirectory,
        widgets::input_item(&form.logging_dir, focus == ConfigField::LoggingDirectory),
    );

    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index);
}

fn login_entry_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::LoginEntry;
    let available = crate::auth::bundled_credentials().is_some();

    let mut spans = vec![widgets::focus_span(focused)];
    if !available {
        spans.push(Span::styled(
            LOGIN_UNAVAILABLE,
            Style::default().fg(TEXT_FAINT),
        ));
    } else {
        match &form.login_state {
            AuthLoginState::LoggedOut => {
                spans.push(Span::styled(LOGIN_LOG_IN, focused_label(focused)));
            }
            AuthLoginState::InProgress(_) => {
                spans.push(Span::styled(LOGIN_LOGGING_IN, Style::default().fg(WARNING)));
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
        Style::default().fg(TEXT_FAINT)
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
            Style::default().fg(TEXT_FAINT),
        )),
        AuthLoginState::InProgress(step) if !step.is_empty() => Some((
            step.clone(),
            Style::default().fg(WARNING).add_modifier(Modifier::ITALIC),
        )),
        AuthLoginState::InProgress(_) => None,
        AuthLoginState::LoggedIn => Some((
            STATUS_LOGGED_IN.to_string(),
            Style::default().fg(SUCCESS).add_modifier(Modifier::BOLD),
        )),
    };

    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            LOGIN_HEADER,
            Style::default().fg(ACCENT_ALT).add_modifier(Modifier::BOLD),
        ),
    ];
    if let Some((text, style)) = status {
        spans.push(" ".into());
        spans.push(Span::styled(text, style));
    }
    ListItem::new(Line::from(spans))
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
