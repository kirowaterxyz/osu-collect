use crate::{
    app::{AuthLoginState, ConfigField, ConfigTab},
    config::{LogFormat, LogLevel},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
};

use super::{ConfigView, components};

pub fn render(frame: &mut Frame, area: Rect, view: ConfigView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &ConfigTab) {
    // 0: section_header("download")
    // 1: help_item
    // 2: DownloadSkipExisting
    // 3: DownloadThreads
    // 4: DownloadNoVideo
    // 5: DownloadVerifyZipEocd
    // 6: spacer
    // 7: section_header("mirrors")
    // 8: help_item
    // 9: MirrorNerinyan
    // 10: MirrorCatboyCentral
    // 11: MirrorCatboyUs
    // 12: MirrorCatboyAsia
    // 13: MirrorOsuDirect
    // 14: MirrorSayobot
    // 15: MirrorNekoha
    // 16: MirrorCustomUrl
    // 17: spacer
    // 18: section_header("logging")
    // 19: LoggingEnabled
    // 20: LoggingLevel
    // 21: LoggingFormat
    // 22: LoggingDirectory
    // 23: spacer
    // 24: section_header("osu! login")
    // 25: auth_status_item
    // 26: LoginEntry
    // 27: LogoutEntry
    let items = vec![
        components::section_header("download"),
        components::help_item("defaults used by home and updates downloads"),
        components::row_item(
            "skip existing files",
            Some("keep files already on disk"),
            form.skip_existing,
            form.focus == ConfigField::DownloadSkipExisting,
        ),
        components::input_item(&form.threads, form.focus == ConfigField::DownloadThreads),
        components::row_item(
            "skip videos",
            Some("smaller downloads"),
            form.no_video,
            form.focus == ConfigField::DownloadNoVideo,
        ),
        components::row_item(
            "verify .osz integrity",
            Some("reject truncated archives"),
            form.verify_zip_eocd,
            form.focus == ConfigField::DownloadVerifyZipEocd,
        ),
        components::spacer(),
        components::section_header("mirrors"),
        components::help_item("space toggles mirrors; custom URL must contain {id}"),
        components::row_item(
            "nerinyan",
            Some("api.nerinyan.moe"),
            form.nerinyan,
            form.focus == ConfigField::MirrorNerinyan,
        ),
        components::row_item(
            "catboy central",
            Some("catboy.best"),
            form.catboy_central,
            form.focus == ConfigField::MirrorCatboyCentral,
        ),
        components::row_item(
            "catboy us",
            Some("us.catboy.best"),
            form.catboy_us,
            form.focus == ConfigField::MirrorCatboyUs,
        ),
        components::row_item(
            "catboy asia",
            Some("sg.catboy.best"),
            form.catboy_asia,
            form.focus == ConfigField::MirrorCatboyAsia,
        ),
        components::row_item(
            "osu!direct",
            Some("osu.direct"),
            form.osu_direct,
            form.focus == ConfigField::MirrorOsuDirect,
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
        components::spacer(),
        components::section_header("logging"),
        components::toggle_item(
            "enable logging",
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
        components::spacer(),
        components::section_header("osu! login"),
        auth_status_item(&form.login_state),
        login_entry_item(form),
        logout_entry_item(form),
    ];

    let focused_index = match form.focus {
        ConfigField::DownloadSkipExisting => 2,
        ConfigField::DownloadThreads => 3,
        ConfigField::DownloadNoVideo => 4,
        ConfigField::DownloadVerifyZipEocd => 5,
        ConfigField::MirrorNerinyan => 9,
        ConfigField::MirrorCatboyCentral => 10,
        ConfigField::MirrorCatboyUs => 11,
        ConfigField::MirrorCatboyAsia => 12,
        ConfigField::MirrorOsuDirect => 13,
        ConfigField::MirrorSayobot => 14,
        ConfigField::MirrorNekoha => 15,
        ConfigField::MirrorCustomUrl => 16,
        ConfigField::LoggingEnabled => 19,
        ConfigField::LoggingLevel => 20,
        ConfigField::LoggingFormat => 21,
        ConfigField::LoggingDirectory => 22,
        ConfigField::LoginEntry => 26,
        ConfigField::LogoutEntry => 27,
    };

    let inner_block = components::panel_block("config");
    let inner = inner_block.inner(area);
    frame.render_widget(inner_block, area);

    let visible_height = inner.height as usize;
    let (start, end) = components::scroll_window(&items, focused_index, visible_height);
    let visible_items = items[start..end].to_vec();

    let list = List::new(visible_items).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn login_entry_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::LoginEntry;
    let available = crate::auth::bundled_credentials().is_some();

    let (label, style) = if !available {
        (
            "login unavailable (no credentials in build)".to_string(),
            Style::default().fg(components::TEXT_FAINT),
        )
    } else {
        match &form.login_state {
            AuthLoginState::LoggedOut => ("log in".to_string(), action_style(focused)),
            AuthLoginState::InProgress(_) => (
                "logging in…".to_string(),
                Style::default()
                    .fg(components::WARNING)
                    .add_modifier(Modifier::ITALIC),
            ),
            AuthLoginState::LoggedIn => ("re-login".to_string(), action_style(focused)),
        }
    };

    ListItem::new(Line::from(vec![
        components::focus_span(focused),
        Span::styled(label, style),
    ]))
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
        components::focus_span(focused),
        Span::styled("log out".to_string(), style),
    ]))
}

fn action_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .fg(components::TEXT_MUTED)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(components::TEXT_MUTED)
    }
}

fn auth_status_item(state: &AuthLoginState) -> ListItem<'static> {
    let (prefix, text, style) = match state {
        AuthLoginState::LoggedOut => (
            components::FOCUS_PAD,
            "> not logged in".to_string(),
            Style::default().fg(components::TEXT_FAINT),
        ),
        AuthLoginState::InProgress(step) => (
            components::FOCUS_PAD,
            format!("> {step}"),
            Style::default()
                .fg(components::WARNING)
                .add_modifier(Modifier::ITALIC),
        ),
        AuthLoginState::LoggedIn => (
            components::FOCUS_PAD,
            "> logged in".to_string(),
            Style::default()
                .fg(components::ACCENT)
                .add_modifier(Modifier::BOLD),
        ),
    };
    ListItem::new(Line::from(vec![
        Span::raw(prefix),
        Span::styled(text, style),
    ]))
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
