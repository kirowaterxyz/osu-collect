use crate::app::Banner;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{bg, danger, text_dim, text_faint, warning};

const ACTION_DISK: &str = "[d] change output dir";

/// Render a single banner row into `area`.
///
/// Uses `warning` background for `DiskLow` and `danger` background for
/// `DiskFull`. The action hint is shown inline.
pub fn render_banner(frame: &mut Frame, area: Rect, banner: &Banner) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let (bg_color, text_color, label) = banner_style_and_text(banner);

    let line = Line::from(vec![
        Span::styled(" ", Style::default().bg(bg_color)),
        Span::styled(label, Style::default().fg(text_color).bg(bg_color)),
        Span::styled(" — ", Style::default().fg(text_faint()).bg(bg_color)),
        Span::styled(
            ACTION_DISK,
            Style::default()
                .fg(text_dim())
                .bg(bg_color)
                .add_modifier(Modifier::BOLD),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line).style(Style::default().bg(bg_color)),
        area,
    );
}

/// Render one row per banner. Returns the number of rows consumed (= banners.len()).
pub fn render_banners(frame: &mut Frame, area: Rect, banners: &[Banner]) {
    if banners.is_empty() || area.height == 0 {
        return;
    }

    for (i, banner) in banners.iter().enumerate() {
        let row_y = area.y + i as u16;
        if row_y >= area.y + area.height {
            break;
        }
        let row = Rect {
            x: area.x,
            y: row_y,
            width: area.width,
            height: 1,
        };
        render_banner(frame, row, banner);
    }
}

/// How many rows `banners` will consume.
pub fn banner_height(banners: &[Banner]) -> u16 {
    banners.len() as u16
}

fn banner_style_and_text(
    banner: &Banner,
) -> (ratatui::style::Color, ratatui::style::Color, String) {
    match banner {
        Banner::DiskFull { free_bytes } => {
            let label = disk_label("disk critical", *free_bytes);
            (danger(), bg(), label)
        }
        Banner::DiskLow { free_bytes } => {
            let label = disk_label("disk low", *free_bytes);
            (warning(), bg(), label)
        }
    }
}

fn disk_label(kind: &str, free_bytes: u64) -> String {
    let mut s = String::with_capacity(32);
    s.push_str(kind);
    s.push_str(": ");
    s.push_str(&format_free_space(free_bytes));
    s
}

fn format_free_space(bytes: u64) -> String {
    const TB: f64 = 1024.0 * 1024.0 * 1024.0 * 1024.0;
    const GB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MB: f64 = 1024.0 * 1024.0;
    const KB: f64 = 1024.0;
    let f = bytes as f64;
    if f >= TB {
        format!("{:.1} TB free", f / TB)
    } else if f >= GB {
        format!("{:.1} GB free", f / GB)
    } else if f >= MB {
        format!("{:.1} MB free", f / MB)
    } else if f >= KB {
        format!("{:.0} KB free", f / KB)
    } else {
        format!("{bytes} B free")
    }
}
