use std::borrow::Cow;

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

use super::{ACCENT, ACCENT_ALT, LINE, LINE_SOFT, TEXT_FAINT};

const BRAND: &str = " osu-collect ";
const VERSION: &str = concat!(" v", env!("CARGO_PKG_VERSION"), " ");

pub fn render<'t>(frame: &mut Frame, area: Rect, tabs: &[Cow<'t, str>], active: usize) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    let version_width = VERSION.len() as u16;
    let brand_width = BRAND.len() as u16;

    let layout = Layout::horizontal([
        Constraint::Length(brand_width),
        Constraint::Min(0),
        Constraint::Length(version_width),
    ])
    .split(area);

    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(
            BRAND,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ))),
        layout[0],
    );

    let mut spans: Vec<Span<'t>> = Vec::with_capacity(tabs.len() * 3);
    spans.push(Span::styled("  ", Style::default().fg(LINE)));
    for (index, title) in tabs.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  │  ", Style::default().fg(LINE_SOFT)));
        }
        let style = if index == active {
            Style::default().fg(ACCENT_ALT).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(TEXT_FAINT)
        };
        spans.push(Span::styled(title.clone(), style));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Left),
        layout[1],
    );

    frame.render_widget(
        Paragraph::new(VERSION)
            .style(Style::default().fg(TEXT_FAINT))
            .alignment(Alignment::Right),
        layout[2],
    );
}
