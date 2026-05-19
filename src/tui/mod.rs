pub mod terminal;

mod config;
mod download;
mod footer;
mod header;
mod home;
mod updates;
mod widgets;

use crate::app::App;
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use osu_downloader::MirrorKind;
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::Block,
};
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

pub const ACCENT: Color = Color::Rgb(67, 171, 229);
pub const ACCENT_ALT: Color = Color::Rgb(217, 119, 87);
pub const INFO: Color = Color::Rgb(116, 199, 236);
pub const SUCCESS: Color = Color::Rgb(166, 227, 161);
pub const WARNING: Color = Color::Rgb(249, 226, 175);
pub const DANGER: Color = Color::Rgb(243, 139, 168);
pub const TEXT: Color = Color::Rgb(205, 214, 244);
pub const TEXT_MUTED: Color = Color::Rgb(186, 194, 222);
pub const TEXT_DIM: Color = Color::Rgb(166, 173, 200);
pub const TEXT_FAINT: Color = Color::Rgb(127, 132, 156);
pub const LINE: Color = Color::Rgb(69, 71, 90);
pub const LINE_SOFT: Color = Color::Rgb(49, 50, 68);
pub const BG: Color = Color::Rgb(30, 30, 46);
pub const BG_RAISED: Color = Color::Rgb(24, 24, 37);

pub const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn spinner_char(tick: u64) -> char {
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

pub const HELP_CUSTOM_MIRROR: &str = "must contain {id}";

pub fn eyebrow() -> Style {
    Style::default().fg(TEXT_FAINT).add_modifier(Modifier::BOLD)
}

pub fn focused_label(focused: bool) -> Style {
    if focused {
        Style::default().fg(TEXT).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(TEXT_MUTED)
    }
}

pub fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width == 0 || area.height == 0 {
        return;
    }

    frame.render_widget(Block::default().style(Style::default().bg(BG)), area);

    let compact = area.height < 14;
    let chunks: Vec<_> = if compact {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .split(area)
        .to_vec()
    } else {
        Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .split(area)
        .to_vec()
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
}

#[cfg(test)]
#[path = "../../tests/unit/tui.rs"]
mod tests;
