use std::borrow::Cow;

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
    HELP_CUSTOM_MIRROR, accent_alt, bg_raised, mirror_label, success, text_faint, text_muted,
    warning,
};
use osu_downloader::MirrorKind;

const PANEL_TITLE: &str = " CONFIG ";

const SECTION_DISPLAY: &str = "display";
const SECTION_DOWNLOAD: &str = "download";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_LOGGING: &str = "logging";

const LABEL_THEME: &str = "theme";

const LABEL_SKIP_VIDEOS: &str = "skip videos";
const LABEL_VERIFY_INTEGRITY: &str = "verify .osz integrity";
const LABEL_RETRY_FAILED: &str = "retry failed on download";
const LABEL_LOGGING_ENABLED: &str = "enable logging";
const LABEL_LOGGING_LEVEL: &str = "log level";
const LABEL_LOGGING_FORMAT: &str = "log format";

const CHIP_UNAVAILABLE: &str = " login unavailable · no credentials in build ";
const CHIP_LOGGED_OUT: &str = " signed out";
const CHIP_LOGGED_IN: &str = " signed in";
const CHIP_ACTION_LOGIN: &str = " · log in ";
const CHIP_ACTION_LOGOUT: &str = " · log out ";
const CHIP_ACTION_CANCEL: &str = " · cancel";
const CHIP_LOGGING_IN: &str = " logging in… ";

const THEME_MODE_LABELS: &[&str] = &["auto", "truecolor", "16-color", "colorblind-safe"];

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
const LOG_FORMATS: &[&str] = &["compact", "pretty"];
const ARCHIVE_VALIDATION_LABELS: &[&str] = &["off", "basic", "strict"];
const RETRY_FAILED_LABELS: &[&str] = &["ask", "yes", "no"];

const HELP_VERIFY_STRICT: &str = "strict mode may reject beatmaps that osu! would still accept";
const HELP_VERIFY_INTEGRITY: &str =
    "off skips checks; basic verifies headers; strict also checks eocd footer";
const HELP_RETRY_FAILED: &str =
    "ask: prompt before each download · yes: always retry · no: never retry";

pub fn render(frame: &mut Frame, area: Rect, form: &ConfigTab) -> Option<(u16, u16)> {
    let show_chrome = area.height >= super::COMPACT_HEIGHT;
    let items = build_config_items(form, show_chrome);

    let cursor_col = form.focused_input().map(widgets::input_cursor_col);
    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index, cursor_col)
}

/// Builds the config form item list. `show_chrome` gates the decorative section
/// headers, spacers, and focus-conditional help lines; the focusable field rows
/// are identical in both modes so the field list lives here once.
///
/// Compact mode (`show_chrome == false`) strips only that chrome — every field
/// stays focusable and navigable.
fn build_config_items(form: &ConfigTab, show_chrome: bool) -> widgets::FormItems<ConfigField> {
    let focus = form.focus;
    let mut items = widgets::FormItems::new(focus);

    items.push_focusable(ConfigField::AuthChip, auth_chip_item(form));
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(SECTION_DISPLAY));
    }

    items.push_focusable(
        ConfigField::Theme,
        widgets::cycle_item(
            LABEL_THEME,
            THEME_MODE_LABELS,
            theme_mode_label(form.theme),
            focus == ConfigField::Theme,
        ),
    );
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(SECTION_DOWNLOAD));
    }

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
            Some(widgets::bool_label(form.no_video)),
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
    if show_chrome && focus == ConfigField::DownloadArchiveValidation {
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
    if show_chrome && focus == ConfigField::RetryFailedOnDownload {
        items.push(widgets::help_item(HELP_RETRY_FAILED));
    }
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(SECTION_MIRRORS));
    }

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
    if show_chrome && focus == ConfigField::MirrorCustomUrl {
        items.push(widgets::help_item(HELP_CUSTOM_MIRROR));
    }
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(SECTION_LOGGING));
    }

    items.push_focusable(
        ConfigField::LoggingEnabled,
        widgets::row_item(
            LABEL_LOGGING_ENABLED,
            Some(widgets::bool_label(form.logging_enabled)),
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

    items
}

/// Renders the auth state chip: a single styled row at the top of the config tab.
///
/// - Signed in:   ` signed in · log out `
/// - Signed out:  ` signed out · log in `
/// - In progress: ` logging in… · cancel`
/// - Unavailable: ` login unavailable · no credentials in build `
fn auth_chip_item(form: &ConfigTab) -> ListItem<'static> {
    let focused = form.focus == ConfigField::AuthChip;
    let chip_bg = Style::default().bg(bg_raised());
    let action_style = chip_bg
        .fg(if focused { accent_alt() } else { text_muted() })
        .add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        });

    let available = crate::auth::bundled_credentials().is_some();
    if !available {
        return ListItem::new(Line::from(Span::styled(
            CHIP_UNAVAILABLE,
            chip_bg.fg(text_faint()),
        )));
    }

    let spans: Vec<Span<'static>> = match &form.login_state {
        AuthLoginState::LoggedOut => vec![
            Span::styled(CHIP_LOGGED_OUT, chip_bg.fg(text_faint())),
            Span::styled(CHIP_ACTION_LOGIN, action_style),
        ],
        AuthLoginState::InProgress(step) => {
            let label: Cow<'static, str> = if step.is_empty() {
                CHIP_LOGGING_IN.into()
            } else {
                format!(" {step} ").into()
            };
            vec![
                Span::styled(label, chip_bg.fg(warning()).add_modifier(Modifier::ITALIC)),
                Span::styled(CHIP_ACTION_CANCEL, action_style),
                Span::styled(" ", chip_bg),
            ]
        }
        AuthLoginState::LoggedIn => vec![
            Span::styled(
                CHIP_LOGGED_IN,
                chip_bg.fg(success()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(CHIP_ACTION_LOGOUT, action_style),
        ],
    };

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
        ThemeMode::Default => "truecolor",
        ThemeMode::Sixteen => "16-color",
        ThemeMode::ColorblindSafe => "colorblind-safe",
    }
}
