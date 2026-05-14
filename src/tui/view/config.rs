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
    // 0:  section_header("osu! login")
    // 1:  auth_status_item
    // 2:  LoginEntry
    // 3:  LogoutEntry
    // 4:  spacer
    // 5:  section_header("download")
    // 6:  DownloadThreads
    // 7:  help_item("defaults shared with home and updates")
    // 8:  DownloadSkipExisting
    // 9:  DownloadNoVideo
    // 10: DownloadVerifyZipEocd
    // 11: spacer
    // 12: section_header("mirrors")
    // 13: MirrorOsuDirect
    // 14: MirrorNerinyan
    // 15: MirrorSayobot
    // 16: MirrorNekoha
    // 17: MirrorCatboyCentral
    // 18: MirrorCatboyUs
    // 19: MirrorCatboyAsia
    // 20: MirrorCustomUrl
    // 21: help_item("must contain {id}")
    // 22: spacer
    // 23: section_header("logging")
    // 24: LoggingEnabled
    // 25: LoggingLevel
    // 26: LoggingFormat
    // 27: LoggingDirectory
    let items = vec![
        components::disclosure_row("default settings and config options", "", false, false),
        components::spacer(),
        components::section_header("osu! login"),
        auth_status_item(&form.login_state),
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
        components::input_item(
            &form.custom_mirror,
            form.focus == ConfigField::MirrorCustomUrl,
        ),
        components::help_item("must contain {id}"),
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
        ConfigField::MirrorCatboyCentral => 18,
        ConfigField::MirrorCatboyUs => 19,
        ConfigField::MirrorCatboyAsia => 20,
        ConfigField::MirrorCustomUrl => 21,
        ConfigField::LoggingEnabled => 25,
        ConfigField::LoggingLevel => 26,
        ConfigField::LoggingFormat => 27,
        ConfigField::LoggingDirectory => 28,
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
    components::focused_label_style(focused)
}

fn auth_status_item(state: &AuthLoginState) -> ListItem<'static> {
    let (text, style) = match state {
        AuthLoginState::LoggedOut => (
            "status: not logged in".to_string(),
            Style::default().fg(components::TEXT_FAINT),
        ),
        AuthLoginState::InProgress(step) => (
            format!("status: {step}"),
            Style::default()
                .fg(components::WARNING)
                .add_modifier(Modifier::ITALIC),
        ),
        AuthLoginState::LoggedIn => (
            "status: logged in".to_string(),
            Style::default()
                .fg(components::SUCCESS)
                .add_modifier(Modifier::BOLD),
        ),
    };
    ListItem::new(Line::from(vec![
        Span::raw(components::FOCUS_PAD),
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
