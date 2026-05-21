pub mod terminal;
pub mod theme;

mod config;
mod download;
mod footer;
mod header;
mod home;
pub(crate) mod modal;
mod updates;
mod widgets;

pub use theme::{Theme, init_theme, theme};

use crate::app::App;
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use osu_downloader::MirrorKind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::Block,
};
use std::rc::Rc;
use std::sync::LazyLock;

/// App-side display label for a mirror. Distinct from `MirrorKind::label()`
/// (which the lib uses for its own defaults) — this carries the TUI's
/// lowercase + branded styling (`osu!direct`).
pub fn mirror_label(kind: MirrorKind) -> &'static str {
    match kind {
        MirrorKind::OsuDirect => "osu!direct",
        MirrorKind::Nerinyan => "nerinyan",
        MirrorKind::Sayobot => "sayobot",
        MirrorKind::Nekoha => "nekoha",
        MirrorKind::Custom => "custom",
    }
}

pub(crate) const GLYPH_H_LINE: &str = "─";
pub(crate) const GLYPH_BLOCK: &str = "█";
pub(crate) const GLYPH_SHADE: &str = "░";
pub(crate) const GLYPH_SPACE: &str = " ";

const MAX_FILL_WIDTH: usize = 256;

pub(crate) static FILL_H_LINE: LazyLock<String> =
    LazyLock::new(|| GLYPH_H_LINE.repeat(MAX_FILL_WIDTH));
pub(crate) static FILL_BLOCK: LazyLock<String> =
    LazyLock::new(|| GLYPH_BLOCK.repeat(MAX_FILL_WIDTH));
pub(crate) static FILL_SHADE: LazyLock<String> =
    LazyLock::new(|| GLYPH_SHADE.repeat(MAX_FILL_WIDTH));
pub(crate) static FILL_SPACE: LazyLock<String> =
    LazyLock::new(|| GLYPH_SPACE.repeat(MAX_FILL_WIDTH));

/// Returns a `&str` slice of `n` repetitions of `glyph` backed by `buf`.
///
/// When `n ≤ MAX_FILL_WIDTH` this is a zero-alloc slice into the pre-built
/// static buffer.  For wider terminals it falls back to `glyph.repeat(n)` so
/// behaviour is always correct.
pub(crate) fn glyph_fill<'a>(
    buf: &'a LazyLock<String>,
    glyph: &'static str,
    n: usize,
) -> std::borrow::Cow<'a, str> {
    if n == 0 {
        return std::borrow::Cow::Borrowed("");
    }
    let end = n * glyph.len();
    if n <= MAX_FILL_WIDTH {
        let s = buf.as_str();
        debug_assert!(
            s.is_char_boundary(end),
            "glyph_fill: {end} is not a char boundary"
        );
        std::borrow::Cow::Borrowed(&s[..end])
    } else {
        std::borrow::Cow::Owned(glyph.repeat(n))
    }
}

// Palette accessors — always go through the process-wide theme so that the
// selected variant (truecolor / 16-color / colorblind-safe) is respected.
// Internal modules import these functions; external callers use `theme()`.
pub(crate) fn accent() -> Color {
    theme().accent
}
pub(crate) fn accent_alt() -> Color {
    theme().accent_alt
}
pub(crate) fn info() -> Color {
    theme().info
}
pub(crate) fn success() -> Color {
    theme().success
}
pub(crate) fn warning() -> Color {
    theme().warning
}
pub(crate) fn danger() -> Color {
    theme().danger
}
pub(crate) fn text() -> Color {
    theme().text
}
pub(crate) fn text_muted() -> Color {
    theme().text_muted
}
pub(crate) fn text_dim() -> Color {
    theme().text_dim
}
pub(crate) fn text_faint() -> Color {
    theme().text_faint
}
pub(crate) fn line() -> Color {
    theme().line
}
pub(crate) fn line_soft() -> Color {
    theme().line_soft
}
pub(crate) fn bg() -> Color {
    theme().bg
}
pub(crate) fn bg_raised() -> Color {
    theme().bg_raised
}

pub const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn spinner_char(tick: u64) -> char {
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

pub const HELP_CUSTOM_MIRROR: &str = "must contain {id}";

pub fn eyebrow() -> Style {
    Style::default()
        .fg(text_faint())
        .add_modifier(Modifier::BOLD)
}

pub fn focused_label(focused: bool) -> Style {
    if focused {
        Style::default().fg(text()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(text_muted())
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(Block::default().style(Style::default().bg(bg())), area);

    let compact = area.height < 14;
    let chunks: Rc<[_]> = if compact {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)
    } else {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area)
    };

    let (header_area, content_area, footer_area) = if compact {
        (chunks[0], chunks[1], chunks[2])
    } else {
        widgets::render_separator(frame, chunks[1]);
        widgets::render_separator(frame, chunks[3]);
        (chunks[0], chunks[2], chunks[4])
    };

    let tabs = app.tab_titles();
    header::render(frame, header_area, &tabs, app.active_tab());

    match app.active_tab() {
        HOME_TAB_INDEX => home::render(frame, content_area, &app.home),
        UPDATES_TAB_INDEX => updates::render(frame, content_area, &app.updates),
        CONFIG_TAB_INDEX => config::render(frame, content_area, &app.config),
        tab => match app.download_for_tab(tab) {
            Some(page) => download::render(frame, content_area, page, app.tick_count),
            None => home::render(frame, content_area, &app.home),
        },
    }

    footer::render(frame, footer_area, app);

    if app.help_open {
        modal::render_help_overlay(frame, area);
    }
}

#[cfg(test)]
#[path = "../../tests/unit/tui.rs"]
mod tests;
