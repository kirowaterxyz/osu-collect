use super::{AppView, components};
use crate::app::{
    ConfigField, HomeTab, MessageKind, UpdatesField, UpdatesTab, messages::AppMessage,
};
use crate::config::constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
};

// braille spinner frames — 80ms/frame in cloudy-ui terminal spec
const SPINNER_FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub struct FooterView<'a> {
    pub message: Option<&'a AppMessage>,
    pub quit_prompt: bool,
    pub hint: String,
    /// monotonically increasing tick count for spinner animation
    pub tick: u64,
}

impl<'a> FooterView<'a> {
    pub fn for_tab(view: &'a AppView<'a>) -> Self {
        let tick = view.tick_count;
        match view.active_tab {
            HOME_TAB_INDEX => Self {
                message: view.home.form.message.as_ref(),
                quit_prompt: view.home.form.quit_prompt,
                hint: home_hint(view.home.form),
                tick,
            },
            UPDATES_TAB_INDEX => Self {
                message: view.updates.form.message.as_ref(),
                quit_prompt: false,
                hint: updates_hint(view.updates.form),
                tick,
            },
            CONFIG_TAB_INDEX => Self {
                message: view.config.form.message.as_ref(),
                quit_prompt: view.config.quit_prompt,
                hint: config_hint(view.config.form.focus),
                tick,
            },
            _ => Self {
                message: None,
                quit_prompt: false,
                hint: "↑↓ scroll threads  ·  q quit/cancel".to_string(),
                tick,
            },
        }
    }
}

fn join_hint(segments: &[&str]) -> String {
    segments.join("  ·  ")
}

fn home_hint(form: &HomeTab) -> String {
    let mut segments: Vec<&str> = vec!["↑↓ move"];
    if !form.focus.is_text_input() {
        segments.push("space toggle");
    }
    segments.push("enter download");
    segments.push("tab next");
    segments.push("q quit");
    join_hint(&segments)
}

fn updates_hint(form: &UpdatesTab) -> String {
    let in_list = form.selection.in_collection_list || form.selection.in_beatmap_list;
    let mut segments: Vec<&str> = Vec::new();

    if in_list {
        segments.push("↑↓ scroll");
        segments.push("space toggle");
        segments.push("a all");
        segments.push("d none");
        segments.push("esc back");
        return join_hint(&segments);
    }

    segments.push("↑↓ move");
    match form.selection.focus {
        UpdatesField::ClientType => segments.push("space switch"),
        UpdatesField::Collections | UpdatesField::BeatmapList => segments.push("space open"),
        UpdatesField::OsuPath => {}
    }
    if form.can_recheck_failed_maps() {
        segments.push("r recheck failed");
    }
    if form.selected_beatmap_count() > 0 {
        segments.push("enter download");
    }
    segments.push("q quit");
    join_hint(&segments)
}

fn config_hint(focus: ConfigField) -> String {
    let mut segments: Vec<&str> = vec!["↑↓ move"];
    match focus {
        ConfigField::LoginEntry | ConfigField::LogoutEntry => segments.push("space confirm"),
        field if field.is_text_input() => {}
        _ => segments.push("space change"),
    }
    segments.push("s save");
    segments.push("q quit");
    join_hint(&segments)
}

pub fn render(frame: &mut Frame, area: Rect, view: FooterView) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    if view.quit_prompt {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled(" ⚠ ", Style::default().fg(components::WARNING)),
                Span::styled(
                    "press q again to quit — active downloads will stop",
                    Style::default().fg(components::TEXT_DIM),
                ),
            ])),
            area,
        );
        return;
    }

    if let Some(msg) = view.message {
        let line = build_message_line(msg, view.tick);
        frame.render_widget(Paragraph::new(line), area);
        return;
    }

    frame.render_widget(Paragraph::new(build_hint_line(&view.hint)), area);
}

fn build_message_line(msg: &AppMessage, tick: u64) -> Line<'static> {
    match msg.kind {
        MessageKind::Loading => {
            let spinner_char = SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()];
            Line::from(vec![
                Span::styled(
                    format!(" {spinner_char} "),
                    Style::default()
                        .fg(components::ACCENT)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    msg.text.trim_start().to_string(),
                    Style::default().fg(components::TEXT_MUTED),
                ),
            ])
        }
        MessageKind::Info => Line::from(vec![
            Span::raw(" "),
            components::status_pill("info", components::INFO),
            Span::raw(" "),
            Span::styled(
                msg.text.trim_start().to_string(),
                Style::default().fg(components::TEXT_MUTED),
            ),
        ]),
        MessageKind::Error => Line::from(vec![
            Span::raw(" "),
            components::status_pill("error", components::DANGER),
            Span::raw(" "),
            Span::styled(
                msg.text.trim_start().to_string(),
                Style::default().fg(components::TEXT_MUTED),
            ),
        ]),
    }
}

fn build_hint_line(hint: &str) -> Line<'static> {
    let mut spans: Vec<Span<'static>> = Vec::new();
    let label_style = Style::default().fg(components::TEXT_FAINT);
    let key_style = Style::default()
        .fg(components::ACCENT)
        .add_modifier(Modifier::BOLD);
    let separator_style = Style::default().fg(components::LINE_SOFT);

    for (segment_index, segment) in hint.split('·').enumerate() {
        let trimmed = segment.trim();
        if trimmed.is_empty() {
            continue;
        }
        if segment_index > 0 {
            spans.push(Span::styled("  │  ", separator_style));
        } else {
            spans.push(Span::raw(" "));
        }
        let mut parts = trimmed.splitn(2, ' ');
        let key = parts.next().unwrap_or("");
        let label = parts.next().unwrap_or("");
        spans.push(Span::styled(key.to_string(), key_style));
        if !label.is_empty() {
            spans.push(Span::styled(format!(" {label}"), label_style));
        }
    }

    Line::from(spans)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spinner_wraps_correctly() {
        // all 10 frames accessible without panic
        for tick in 0u64..30 {
            let frame = SPINNER_FRAMES[tick as usize % SPINNER_FRAMES.len()];
            assert!(SPINNER_FRAMES.contains(&frame));
        }
    }

    #[test]
    fn hint_line_has_key_and_label_spans() {
        let line = build_hint_line("↑↓ move  ·  q quit");
        // should produce spans including the key characters
        let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(full.contains("↑↓"));
        assert!(full.contains("move"));
        assert!(full.contains("q"));
        assert!(full.contains("quit"));
    }
}
