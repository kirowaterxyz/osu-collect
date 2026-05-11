use super::{AppView, components};
use crate::app::{MessageKind, messages::AppMessage};
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::Paragraph,
};

pub struct FooterView<'a> {
    pub message: Option<&'a AppMessage>,
    pub quit_prompt: bool,
    pub hint: &'static str,
}

impl<'a> FooterView<'a> {
    pub fn for_tab(view: &'a AppView<'a>) -> Self {
        match view.active_tab {
            HOME_TAB_INDEX => Self {
                message: view.home.form.message.as_ref(),
                quit_prompt: view.home.form.quit_prompt,
                hint: " ↑↓ navigate · space toggle · enter download · q quit",
            },
            UPDATES_TAB_INDEX => Self {
                message: view.updates.form.message.as_ref(),
                quit_prompt: false,
                hint: " ↑↓ navigate · space expand/toggle · a/d all/none · enter download",
            },
            CONFIG_TAB_INDEX => Self {
                message: view.config.form.message.as_ref(),
                quit_prompt: view.config.quit_prompt,
                hint: " ↑↓ navigate · space/←→ change · s save · q quit",
            },
            _ => Self {
                message: None,
                quit_prompt: false,
                hint: " ↑↓ scroll · q quit",
            },
        }
    }
}

pub fn render(frame: &mut Frame, area: Rect, view: FooterView) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if view.quit_prompt {
        let line = Line::from(vec![
            Span::styled(" ! ", Style::default().fg(components::WARNING)),
            Span::styled(
                "press q again to quit; all downloads will be cancelled.",
                Style::default().fg(components::TEXT_DIM),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    if let Some(msg) = view.message {
        let (glyph, color) = match msg.kind {
            MessageKind::Info => ("✓ ", components::SUCCESS),
            MessageKind::Error => ("✗ ", components::DANGER),
            MessageKind::Loading => ("… ", components::WARNING),
        };
        let line = Line::from(vec![
            Span::styled(format!(" {glyph}"), Style::default().fg(color)),
            Span::styled(
                msg.text.trim_start().to_string(),
                Style::default().fg(color),
            ),
        ]);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    let line = build_hint_line(view.hint);
    frame.render_widget(Paragraph::new(line), area);
}

fn build_hint_line(hint: &'static str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let dim = Style::default().fg(components::TEXT_FAINT);
    let key_style = Style::default().fg(components::ACCENT);
    let sep_style = Style::default().fg(components::LINE_SOFT);

    for (segment_idx, segment) in hint.split('·').enumerate() {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if segment_idx > 0 {
            spans.push(Span::styled("  ·  ", sep_style));
        } else {
            spans.push(Span::raw(" "));
        }
        let mut parts = trimmed.splitn(2, ' ');
        let key = parts.next().unwrap_or("");
        let label = parts.next().unwrap_or("");
        spans.push(Span::styled(key.to_string(), key_style));
        if !label.is_empty() {
            spans.push(Span::styled(format!(" {label}"), dim));
        }
    }

    Line::from(spans)
}
