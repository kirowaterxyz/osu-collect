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
    HELP_CUSTOM_MIRROR, bg_hover, bg_raised, mirror_label, success, text, text_dim, text_faint,
    warning,
};
use osu_downloader::MirrorKind;

const PANEL_TITLE: &str = " CONFIG ";

const SECTION_DISPLAY: &str = "display";
const SECTION_DOWNLOAD: &str = "download";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_LOGGING: &str = "logging";

const LABEL_THEME: &str = "theme";

const LABEL_VIDEO: &str = "video";
const LABEL_VERIFY_INTEGRITY: &str = "verify .osz integrity";
const LABEL_RETRY_FAILED: &str = "retry failed on download";
const LABEL_LOGGING_ENABLED: &str = "enable logging";
const LABEL_LOGGING_LEVEL: &str = "log level";
const LABEL_LOGGING_FORMAT: &str = "log format";

const CHIP_UNAVAILABLE: &str = " login unavailable · no credentials in build ";
const CHIP_LOGGED_OUT: &str = " signed out";
const CHIP_LOGGED_IN: &str = " signed in";
const CHIP_ACTION_LOGIN: &str = "log in";
const CHIP_ACTION_LOGOUT: &str = "log out";
const CHIP_ACTION_CANCEL: &str = "cancel";
const CHIP_LOGGING_IN: &str = " logging in… ";
const CHIP_LOGIN_HINT: &str = "this does nothing (yet)";

const THEME_MODE_LABELS: &[&str] = &["full", "compatible"];

const LOG_LEVELS: &[&str] = &["error", "warn", "info", "debug", "trace"];
const LOG_FORMATS: &[&str] = &["compact", "pretty"];
const ARCHIVE_VALIDATION_LABELS: &[&str] = &["off", "basic", "strict"];
const RETRY_FAILED_LABELS: &[&str] = &["ask", "yes", "no"];

/// State-specific hint for the archive-validation cycle: each describes only
/// what the currently selected mode does.
fn archive_validation_help(mode: ArchiveValidation) -> &'static str {
    match mode {
        ArchiveValidation::Off => "checks only the file is not empty",
        ArchiveValidation::Magic => "verifies archive headers",
        ArchiveValidation::Eocd => {
            "also verifies eocd footer; turn off if many maps fail validation"
        }
    }
}

/// State-specific hint for the retry-failed cycle: each describes only what the
/// currently selected mode does.
fn retry_failed_help(mode: RetryFailedOnDownload) -> &'static str {
    match mode {
        RetryFailedOnDownload::Ask => "prompts before each download",
        RetryFailedOnDownload::Yes => "always retries failed maps",
        RetryFailedOnDownload::No => "never retries failed maps",
    }
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    form: &ConfigTab,
    editing: bool,
) -> Option<(u16, u16)> {
    let show_chrome = area.height >= super::COMPACT_HEIGHT;
    let items = build_config_items(form, show_chrome, editing);

    let cursor_col = editing
        .then(|| form.focused_input().map(widgets::input_cursor_col))
        .flatten();
    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(
        frame,
        area,
        PANEL_TITLE,
        &items,
        focused_index,
        cursor_col,
        true,
        true,
    )
}

/// Builds the config form item list. `show_chrome` gates the decorative section
/// headers, spacers, and focus-conditional help lines; the focusable field rows
/// are identical in both modes so the field list lives here once.
///
/// Compact mode (`show_chrome == false`) strips only that chrome — every field
/// stays focusable and navigable.
fn build_config_items(
    form: &ConfigTab,
    show_chrome: bool,
    editing: bool,
) -> widgets::FormItems<ConfigField> {
    let focus = form.focus;
    let active_section = focus_section(focus);
    let mut items = widgets::FormItems::new(focus);

    items.push_focusable(ConfigField::AuthChip, auth_chip_item(form));
    if show_chrome && focus == ConfigField::AuthChip && crate::auth::bundled_credentials().is_some()
    {
        items.push(widgets::help_item(CHIP_LOGIN_HINT));
    }
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(
            SECTION_DISPLAY,
            active_section == Some(SECTION_DISPLAY),
        ));
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
        items.push(widgets::section_header(
            SECTION_MIRRORS,
            active_section == Some(SECTION_MIRRORS),
        ));
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
        widgets::input_item(
            &form.custom_mirror,
            focus == ConfigField::MirrorCustomUrl,
            editing,
        ),
    );
    if show_chrome && focus == ConfigField::MirrorCustomUrl {
        items.push(widgets::help_item(HELP_CUSTOM_MIRROR));
    }
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(
            SECTION_DOWNLOAD,
            active_section == Some(SECTION_DOWNLOAD),
        ));
    }

    items.push_focusable(
        ConfigField::DownloadVideo,
        widgets::row_item(
            LABEL_VIDEO,
            None,
            form.video,
            focus == ConfigField::DownloadVideo,
        ),
    );
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
        ConfigField::DownloadArchiveValidation,
        widgets::cycle_item(
            LABEL_VERIFY_INTEGRITY,
            ARCHIVE_VALIDATION_LABELS,
            archive_validation_label(form.archive_validation),
            focus == ConfigField::DownloadArchiveValidation,
        ),
    );
    if show_chrome && focus == ConfigField::DownloadArchiveValidation {
        items.push(widgets::help_item(archive_validation_help(
            form.archive_validation,
        )));
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
        items.push(widgets::help_item(retry_failed_help(
            form.retry_failed_on_download,
        )));
    }
    if show_chrome {
        items.push(widgets::spacer());
        items.push(widgets::section_header(
            SECTION_LOGGING,
            active_section == Some(SECTION_LOGGING),
        ));
    }

    items.push_focusable(
        ConfigField::LoggingEnabled,
        widgets::row_item(
            LABEL_LOGGING_ENABLED,
            None,
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
        widgets::input_item(
            &form.logging_dir,
            focus == ConfigField::LoggingDirectory,
            editing,
        ),
    );

    items
}

/// The section a focused field belongs to, driving the active-section header
/// cue. `AuthChip` sits above every header, so it maps to no section.
fn focus_section(field: ConfigField) -> Option<&'static str> {
    use ConfigField::*;
    Some(match field {
        AuthChip => return None,
        Theme => SECTION_DISPLAY,
        MirrorOsuDirect | MirrorNerinyan | MirrorSayobot | MirrorNekoha | MirrorCustomUrl => {
            SECTION_MIRRORS
        }
        DownloadThreads | DownloadVideo | DownloadArchiveValidation | RetryFailedOnDownload => {
            SECTION_DOWNLOAD
        }
        LoggingEnabled | LoggingLevel | LoggingFormat | LoggingDirectory => SECTION_LOGGING,
    })
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

    let available = crate::auth::bundled_credentials().is_some();
    if !available {
        return ListItem::new(Line::from(Span::styled(
            CHIP_UNAVAILABLE,
            chip_bg.fg(text_faint()),
        )));
    }

    // State segment (semantic when charged, TEXT_DIM neutral) then a 2-space
    // gap on the chip fill, then the action segment — no mid-dot separator.
    let (state, action_label) = match &form.login_state {
        AuthLoginState::LoggedOut => (
            Span::styled(CHIP_LOGGED_OUT, chip_bg.fg(text_dim())),
            CHIP_ACTION_LOGIN,
        ),
        AuthLoginState::InProgress(step) => {
            let label: Cow<'static, str> = if step.is_empty() {
                CHIP_LOGGING_IN.into()
            } else {
                format!(" {step} ").into()
            };
            // No italic — cloudy-tui reserves italic for panel/modal titles.
            (
                Span::styled(label, chip_bg.fg(warning())),
                CHIP_ACTION_CANCEL,
            )
        }
        AuthLoginState::LoggedIn => (
            Span::styled(
                CHIP_LOGGED_IN,
                chip_bg.fg(success()).add_modifier(Modifier::BOLD),
            ),
            CHIP_ACTION_LOGOUT,
        ),
    };

    ListItem::new(Line::from(vec![
        state,
        Span::styled("  ", chip_bg),
        chip_action_span(action_label, focused, chip_bg),
    ]))
}

/// The chip's inline action segment.
///
/// Focused: highlighted like a selected row — `TEXT + bold` on `BG_HOVER` (the
/// row-selection tint), 1-space inset. A deliberate departure from the
/// cloudy-tui action-chip spec (an `ACCENT`-fill inverse block) so the auth
/// action reads as the focused row, not a sapphire button. Blurred: `TEXT_DIM`
/// on the chip's `BG_RAISED` fill with a trailing pad cell, no bold.
fn chip_action_span(label: &'static str, focused: bool, chip_bg: Style) -> Span<'static> {
    if focused {
        Span::styled(
            format!(" {label} "),
            Style::default()
                .fg(text())
                .bg(bg_hover())
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(format!("{label} "), chip_bg.fg(text_dim()))
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
        ThemeMode::Full => "full",
        ThemeMode::Compatible => "compatible",
    }
}
