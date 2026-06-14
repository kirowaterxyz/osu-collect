//! Toast rendering (cloudy-tui): a floating stack anchored to the top-right.
//!
//! Each toast is borderless: a 1-cell semantic `┃` bar, then content on a
//! semi-transparent surface (`BG_SUNKEN` blended at 75 % over the cells
//! beneath). Toasts render last in the frame so the buffer below is final.

use ratatui::{
    Frame,
    layout::Rect,
    style::{Color, Modifier, Style},
};

use super::theme::blend;
use super::widgets::truncate_to_width;
use super::{bg, bg_sunken, danger, info, success, text, text_dim, warning};
use crate::app::{ToastLevel, Toasts};

/// Margin from the top and right screen edges.
const TOP_INSET: u16 = 2;
const RIGHT_INSET: u16 = 2;
/// Max box width — wide enough for the app's longest toast copy, still bounded
/// so a toast never spans a wide terminal. Collapses on narrow terminals.
const MAX_WIDTH: u16 = 60;
/// bar (1) + a 1-cell pad on each side of the content.
const CHROME_WIDTH: u16 = 3;
/// Column where content starts inside the box: bar (1) + left pad (1).
const CONTENT_OFFSET: u16 = 2;
/// Heavy vertical left-bar.
const BAR: &str = "┃";
/// Opacity of the sunken surface over whatever sits beneath.
const BLEND_RATIO: f32 = 0.75;

/// Render the toast stack into the full-screen `area`. Newest sits on top.
pub fn render_toasts(frame: &mut Frame, area: Rect, toasts: &Toasts) {
    if toasts.is_empty() || area.width < RIGHT_INSET + CHROME_WIDTH || area.height <= TOP_INSET {
        return;
    }

    let max_width = MAX_WIDTH.min(area.width.saturating_sub(RIGHT_INSET * 2));
    let content_budget = max_width.saturating_sub(CHROME_WIDTH);
    if content_budget == 0 {
        return;
    }

    let bottom = area.y + area.height;
    let mut y = area.y + TOP_INSET;
    for toast in toasts.iter().rev() {
        let (title, title_w) = truncate_to_width(toast.title(), content_budget);
        let detail = toast
            .detail()
            .map(|line| truncate_to_width(line, content_budget));
        let content_w = title_w.max(detail.as_ref().map_or(0, |(_, w)| *w));
        let box_w = content_w + CHROME_WIDTH;
        let box_h = 1 + u16::from(detail.is_some());
        if y + box_h > bottom {
            break; // out of vertical room — drop the rest
        }

        let rect = Rect {
            x: area.x + area.width - RIGHT_INSET - box_w,
            y,
            width: box_w,
            height: box_h,
        };
        draw_toast(
            frame,
            rect,
            toast.level(),
            &title,
            detail.as_ref().map(|(line, _)| line.as_str()),
        );
        y += box_h;
    }
}

fn draw_toast(frame: &mut Frame, rect: Rect, level: ToastLevel, title: &str, detail: Option<&str>) {
    blend_surface(frame, rect);

    let bar_style = Style::default().fg(bar_color(level));
    let title_style = Style::default().fg(text()).add_modifier(Modifier::BOLD);
    let detail_style = Style::default().fg(text_dim());
    let buf = frame.buffer_mut();

    buf.set_string(rect.x, rect.y, BAR, bar_style);
    buf.set_string(rect.x + CONTENT_OFFSET, rect.y, title, title_style);
    if let Some(detail) = detail {
        buf.set_string(rect.x, rect.y + 1, BAR, bar_style);
        buf.set_string(rect.x + CONTENT_OFFSET, rect.y + 1, detail, detail_style);
    }
}

/// Tint every cell of `rect` toward `BG_SUNKEN` at [`BLEND_RATIO`] over whatever
/// sits beneath, and clear the glyph so the surface reads as glass, not ghost
/// text. Content written afterward sets `fg` only, so the blend shows through.
fn blend_surface(frame: &mut Frame, rect: Rect) {
    let base = bg();
    let sunken = bg_sunken();
    let buf = frame.buffer_mut();
    for cy in rect.y..rect.y + rect.height {
        for cx in rect.x..rect.x + rect.width {
            if let Some(cell) = buf.cell_mut((cx, cy)) {
                let under = match cell.bg {
                    Color::Reset => base,
                    other => other,
                };
                cell.set_symbol(" ");
                cell.bg = blend(sunken, under, BLEND_RATIO);
                cell.fg = Color::Reset;
            }
        }
    }
}

fn bar_color(level: ToastLevel) -> Color {
    match level {
        ToastLevel::Success => success(),
        ToastLevel::Info => info(),
        ToastLevel::Warning => warning(),
        ToastLevel::Danger => danger(),
    }
}
