pub mod terminal;

mod config;
pub mod download;
pub mod footer;
mod header;
mod home;
mod updates;
pub mod widgets;

use crate::app::App;
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use ratatui::{
    Frame,
    layout::{Constraint, Layout},
    style::{Color, Modifier, Style},
    widgets::Block,
};

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
pub const INDETERMINATE: Color = Color::Rgb(45, 132, 196);

pub const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub fn spinner_char(tick: u64) -> char {
    SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()]
}

pub const MIRRORS: &[(&str, &str)] = &[
    ("osu!direct", "osu.direct"),
    ("nerinyan", "api.nerinyan.moe"),
    ("sayobot", "dl.sayobot.cn"),
    ("nekoha", "mirror.nekoha.moe"),
];

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
