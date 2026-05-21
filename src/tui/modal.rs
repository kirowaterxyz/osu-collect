//! Reusable modal overlay primitives.
//!
//! # Usage
//!
//! 1. Compute the popup area with [`centered_rect`].
//! 2. Render [`ratatui::widgets::Clear`] over that area to erase the content
//!    behind the popup.
//! 3. Call the specific overlay renderer (e.g. [`render_help_overlay`]).
//!
//! Future modals (retry-confirm, pre-save diff, etc.) follow the same pattern:
//! add a render function here that accepts `frame` and `area`, and call
//! [`centered_rect`] in the draw entry point to position it.

use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding},
};

use super::{ACCENT, ACCENT_ALT, BG_RAISED, LINE, TEXT_DIM, TEXT_FAINT};

/// Returns a [`Rect`] centred in `area` of the requested proportional size.
///
/// `percent_x` and `percent_y` are clamped to `[0, 100]`. The popup is never
/// larger than `area` itself.
pub(crate) fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let px = percent_x.min(100);
    let py = percent_y.min(100);
    let [popup_area] = Layout::vertical([Constraint::Percentage(py)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Percentage(px)])
        .flex(Flex::Center)
        .areas(popup_area);
    popup_area
}

/// Key/action pair for the help overlay.
struct HelpRow {
    key: &'static str,
    action: &'static str,
}

impl HelpRow {
    const fn new(key: &'static str, action: &'static str) -> Self {
        Self { key, action }
    }
}

const GLOBAL: &[HelpRow] = &[
    HelpRow::new("tab / shift+tab", "switch tabs"),
    HelpRow::new("?", "toggle help"),
    HelpRow::new("q / esc", "back / quit"),
];

const NAVIGATION: &[HelpRow] = &[
    HelpRow::new("↑ / ↓", "move / scroll"),
    HelpRow::new("enter", "confirm / activate"),
    HelpRow::new("space", "toggle"),
];

const HOME_TAB: &[HelpRow] = &[HelpRow::new("enter", "start download")];

const UPDATES_TAB: &[HelpRow] = &[
    HelpRow::new("enter", "download selected"),
    HelpRow::new("a", "select all"),
    HelpRow::new("d", "select none"),
    HelpRow::new("r", "recheck failed"),
];

const CONFIG_TAB: &[HelpRow] = &[
    HelpRow::new("s", "save config"),
    HelpRow::new("enter", "log in / log out"),
];

/// Renders a centred keybindings overlay.
///
/// Call this after all other tab content and the footer have been drawn —
/// it clears the area it occupies and draws on top.
pub(crate) fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let popup_area = centered_rect(58, 72, area);
    frame.render_widget(Clear, popup_area);

    let outer_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(LINE))
        .style(Style::default().bg(BG_RAISED))
        .title(Span::styled(
            " KEYBINDINGS ",
            Style::default()
                .fg(ACCENT_ALT)
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::ITALIC),
        ))
        .padding(Padding::new(1, 1, 0, 0));

    let inner = outer_block.inner(popup_area);
    frame.render_widget(outer_block, popup_area);

    let items = build_help_items();
    frame.render_widget(List::new(items), inner);
}

fn build_help_items() -> Vec<ListItem<'static>> {
    let mut items = Vec::new();
    push_section(&mut items, "global", GLOBAL);
    items.push(spacer());
    push_section(&mut items, "navigation", NAVIGATION);
    items.push(spacer());
    push_section(&mut items, "home", HOME_TAB);
    items.push(spacer());
    push_section(&mut items, "updates", UPDATES_TAB);
    items.push(spacer());
    push_section(&mut items, "config", CONFIG_TAB);
    items.push(spacer());
    items.push(dismiss_hint());
    items
}

fn push_section(items: &mut Vec<ListItem<'static>>, heading: &'static str, rows: &[HelpRow]) {
    items.push(section_heading(heading));
    for row in rows {
        items.push(help_row(row.key, row.action));
    }
}

fn section_heading(label: &'static str) -> ListItem<'static> {
    ListItem::new(Line::from(vec![Span::styled(
        label.to_uppercase(),
        Style::default().fg(ACCENT_ALT).add_modifier(Modifier::BOLD),
    )]))
}

fn help_row(key: &'static str, action: &'static str) -> ListItem<'static> {
    const KEY_WIDTH: usize = 16;
    let pad = KEY_WIDTH.saturating_sub(key.len());
    let mut key_cell = String::with_capacity(KEY_WIDTH + 2);
    key_cell.push_str("  ");
    key_cell.push_str(key);
    for _ in 0..pad {
        key_cell.push(' ');
    }
    ListItem::new(Line::from(vec![
        Span::styled(
            key_cell,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ),
        Span::styled(action, Style::default().fg(TEXT_DIM)),
    ]))
}

fn spacer() -> ListItem<'static> {
    ListItem::new(Line::from(""))
}

fn dismiss_hint() -> ListItem<'static> {
    ListItem::new(Line::from(vec![Span::styled(
        "  press ? or esc to close",
        Style::default().fg(TEXT_FAINT),
    )]))
}
