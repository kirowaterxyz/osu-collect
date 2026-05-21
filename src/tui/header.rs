use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::config::constants::{DISK_DANGER_BYTES, DISK_WARN_BYTES};

use super::{accent, accent_alt, danger, line, line_soft, text_dim, text_faint, warning};

const BRAND: &str = " osu-collect ";
const VERSION: &str = concat!(" v", env!("CARGO_PKG_VERSION"), " ");

const PILL_SEP: &str = " · ";

/// Aggregated state passed into the header renderer.
///
/// Build via [`StatusPill::compute`]; pass `None` when the pill should be hidden.
pub struct StatusPill {
    /// Active-downloading page count (0 means omit the segment).
    pub downloading: usize,
    /// Free bytes on the output filesystem, if available.
    pub disk_free: Option<u64>,
}

impl StatusPill {
    /// Returns `None` when both segments would be empty (no downloads, no disk path).
    pub fn compute(downloading: usize, disk_free: Option<u64>) -> Option<Self> {
        if downloading == 0 && disk_free.is_none() {
            return None;
        }
        Some(Self {
            downloading,
            disk_free,
        })
    }

    /// Formatted pill text and the color each segment should use.
    ///
    /// Returns a list of `(text, color)` pairs — caller renders them as spans.
    pub fn segments(&self) -> Vec<(String, Color)> {
        let mut out: Vec<(String, Color)> = Vec::with_capacity(3);

        if self.downloading > 0 {
            let n = self.downloading;
            let label = if n == 1 {
                "1 downloading".to_string()
            } else {
                let mut s = n.to_string();
                s.push_str(" downloading");
                s
            };
            out.push((label, text_dim()));
        }

        if let Some(free) = self.disk_free {
            if !out.is_empty() {
                out.push((PILL_SEP.to_string(), text_faint()));
            }
            let color = if free < DISK_DANGER_BYTES {
                danger()
            } else if free < DISK_WARN_BYTES {
                warning()
            } else {
                text_dim()
            };
            let label = format_free_space(free);
            out.push((label, color));
        }

        out
    }

    /// Total display width of the pill (sum of all segment char lengths) plus leading space.
    pub fn display_width(&self) -> u16 {
        let segs = self.segments();
        if segs.is_empty() {
            return 0;
        }
        // 1 leading space + char count of all segments (ASCII, so byte len == char count)
        let chars: usize = segs.iter().map(|(s, _)| s.len()).sum();
        (1 + chars) as u16
    }
}

/// Format free bytes as `"1.5 TB free"`, `"45.1 GB free"`, `"234.5 MB free"`, etc.
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

pub fn render<'t>(
    frame: &mut Frame,
    area: Rect,
    tabs: &[Cow<'t, str>],
    active: usize,
    pill: Option<&StatusPill>,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let version_width = VERSION.len() as u16;
    let brand_width = BRAND.len() as u16;
    let pill_width = pill.map(|p| p.display_width()).unwrap_or(0);

    let layout = Layout::horizontal([
        Constraint::Length(brand_width),
        Constraint::Min(0),
        Constraint::Length(pill_width),
        Constraint::Length(version_width),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            BRAND,
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );

    let mut spans: Vec<Span<'t>> = Vec::with_capacity(tabs.len() * 3);
    spans.push(Span::styled("  ", Style::default().fg(line())));
    for (index, title) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  │  ", Style::default().fg(line_soft())));
        }
        let style = if index == active {
            Style::default()
                .fg(accent_alt())
                .add_modifier(Modifier::BOLD | Modifier::UNDERLINED)
        } else {
            Style::default().fg(text_faint())
        };
        spans.push(Span::styled(title.clone(), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        layout[1],
    );

    if let Some(pill) = pill {
        let segs = pill.segments();
        if !segs.is_empty() {
            let mut pill_spans: Vec<Span<'static>> = Vec::with_capacity(segs.len() + 1);
            // Leading space so the pill has breathing room before the version.
            pill_spans.push(Span::styled(" ", Style::default().fg(text_faint())));
            for (text, color) in segs {
                pill_spans.push(Span::styled(text, Style::default().fg(color)));
            }
            frame.render_widget(
                Paragraph::new(Line::from(pill_spans)).alignment(Alignment::Right),
                layout[2],
            );
        }
    }

    frame.render_widget(
        Paragraph::new(VERSION)
            .style(Style::default().fg(text_faint()))
            .alignment(Alignment::Right),
        layout[3],
    );
}

#[cfg(test)]
#[path = "../../tests/unit/tui_header.rs"]
mod tests;
