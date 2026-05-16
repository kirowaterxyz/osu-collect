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

use super::{ConfigView, components};

pub fn render(frame: &mut Frame, area: Rect, view: ConfigView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &ConfigTab) {
    let items = vec![
        components::disclosure_row("default settings and config options", "", false, false),
        components::spacer(),
        login_section_header(&form.login_state),
        login_entry_item(form),
        logout_entry_item(form),
        components::spacer(),
        components::section_header("download"),
        components::input_item(&form.threads, form.focus == ConfigField::DownloadThreads),
        components::row_item(
            "skip existing files",
            None,
            form.skip_existing,
            form.focus == ConfigField::DownloadSkipExisting,
        ),
        components::row_item(
            "skip videos",
            None,
            form.no_video,
            form.focus == ConfigField::DownloadNoVideo,
        ),
        components::row_item(
            "verify .osz integrity",
            None,
            form.verify_zip_eocd,
            form.focus == ConfigField::DownloadVerifyZipEocd,
        ),
        components::spacer(),
        components::section_header("mirrors"),
        components::row_item(
            "osu!direct",
            Some("osu.direct"),
            form.osu_direct,
            form.focus == ConfigField::MirrorOsuDirect,
        ),
        components::row_item(
            "nerinyan",
            Some("api.nerinyan.moe"),
            form.nerinyan,
            form.focus == ConfigField::MirrorNerinyan,
        ),
        components::row_item(
            "sayobot",
            Some("dl.sayobot.cn"),
            form.sayobot,
            form.focus == ConfigField::MirrorSayobot,
        ),
        components::row_item(
            "nekoha",
            Some("mirror.nekoha.moe"),
            form.nekoha,
            form.focus == ConfigField::MirrorNekoha,
        ),
        components::input_item(
            &form.custom_mirror,
            form.focus == ConfigField::MirrorCustomUrl,
        ),
        components::help_item("must contain {id}"),
        components::spacer(),
        components::section_header("logging"),
        components::row_item(
            "enable logging",
            None,
            form.logging_enabled,
            form.focus == ConfigField::LoggingEnabled,
        ),
        components::cycle_item(
            "level",
            &["error", "warn", "info", "debug", "trace"],
            log_level_label(form.logging_level),
            form.focus == ConfigField::LoggingLevel,
        ),
        components::cycle_item(
            "format",
            &["compact", "pretty"],
            log_format_label(form.logging_format),
            form.focus == ConfigField::LoggingFormat,
        ),
        components::input_item(
            &form.logging_dir,
            form.focus == ConfigField::LoggingDirectory,
        ),
    ];

    let focused_index = match form.focus {
        ConfigField::LoginEntry => 2,
        ConfigField::LogoutEntry => 3,
        ConfigField::DownloadThreads => 6,
        ConfigField::DownloadSkipExisting => 8,
        ConfigField::DownloadNoVideo => 9,
        ConfigField::DownloadVerifyZipEocd => 10,
        ConfigField::MirrorOsuDirect => 13,
        ConfigField::MirrorNerinyan => 15,
        ConfigField::MirrorSayobot => 16,
        ConfigField::MirrorNekoha => 17,
        ConfigField::MirrorCustomUrl => 18,
        ConfigField::LoggingEnabled => 22,
        ConfigField::LoggingLevel => 23,
        ConfigField::LoggingFormat => 24,
        ConfigField::LoggingDirectory => 25,
    };

    components::render_scrollable_panel(frame, area, "config", &items, focused_index);
}

fn login_entry_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::LoginEntry;
    let available = crate::auth::bundled_credentials().is_some();

    let mut spans = vec![components::focus_span(focused)];
    if !available {
        spans.push(Span::styled(
            "login unavailable (no credentials in build)",
            Style::default().fg(components::TEXT_FAINT),
        ));
    } else {
        match &form.login_state {
            AuthLoginState::LoggedOut => {
                spans.push(Span::styled("log in", action_style(focused)));
            }
            AuthLoginState::InProgress(_) => {
                spans.push(Span::styled(
                    "logging in...",
                    Style::default().fg(components::WARNING),
                ));
                spans.push(Span::styled(" (cancel?)", action_style(focused)));
            }
            AuthLoginState::LoggedIn => {
                spans.push(Span::styled("re-login", action_style(focused)));
            }
        }
    }

    ListItem::new(Line::from(spans))
}

fn logout_entry_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::LogoutEntry;
    let enabled = matches!(form.login_state, AuthLoginState::LoggedIn);

    let style = if enabled {
        action_style(focused)
    } else {
        Style::default().fg(components::TEXT_FAINT)
    };

    ListItem::new(Line::from(vec![
        components::focus_span(focused && enabled),
        Span::styled("log out".to_string(), style),
    ]))
}

fn action_style(focused: bool) -> Style {
    components::focused_label_style(focused)
}

fn login_section_header(state: &AuthLoginState) -> ListItem<'static> {
    let status = match state {
        AuthLoginState::LoggedOut => Some((
            "logged out".to_string(),
            Style::default().fg(components::TEXT_FAINT),
        )),
        AuthLoginState::InProgress(step) if !step.is_empty() => Some((
            step.clone(),
            Style::default()
                .fg(components::WARNING)
                .add_modifier(Modifier::ITALIC),
        )),
        AuthLoginState::InProgress(_) => None,
        AuthLoginState::LoggedIn => Some((
            "logged in".to_string(),
            Style::default()
                .fg(components::SUCCESS)
                .add_modifier(Modifier::BOLD),
        )),
    };

    let mut spans = vec![
        Span::raw("  "),
        Span::styled(
            "OSU! LOGIN",
            Style::default()
                .fg(components::ACCENT_ALT)
                .add_modifier(Modifier::BOLD),
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
