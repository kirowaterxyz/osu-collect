//! Reusable modal overlay primitives.
//!
//! # Usage
//!
//! 1. Render [`ratatui::widgets::Clear`] over the popup area to erase the
//!    content behind it.
//! 2. Call the specific overlay renderer (e.g. [`render_help_overlay`]).
//!
//! Future modals follow the same pattern: add a render function here that
//! accepts `frame` and `area`, compute the popup rect with inline
//! `Layout::vertical` + `Layout::horizontal`, and call
//! `frame.render_widget(Clear, popup_area)` before drawing.

use crate::app::ConfigDiffEntry;
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding},
};

use super::widgets;
use super::{accent, accent_alt, bg_raised, line, text_dim, text_faint};

const MODAL_TITLE_SAVE: &str = " SAVE CONFIG ";
const SAVE_HINT: &str = "  [enter] save  ·  [esc] cancel";
const NO_CHANGES_HINT: &str = "  [esc] dismiss";

/// Standard inner padding for every modal popup (1 col each side, no rows).
fn modal_padding() -> Padding {
    Padding::new(1, 1, 0, 0)
}

/// Builds the standard bordered modal block: plain border in `line`, raised
/// background, a bold-italic `accent_alt` title, and [`modal_padding`].
///
/// Callers add a scroll indicator via `title_top` afterwards when needed.
fn modal_block(title: &'static str) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(line()))
        .style(Style::default().bg(bg_raised()))
        .title(Span::styled(
            title,
            Style::default()
                .fg(accent_alt())
                .add_modifier(Modifier::BOLD)
                .add_modifier(Modifier::ITALIC),
        ))
        .padding(modal_padding())
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
    HelpRow::new("enter", "toggle / activate / confirm"),
    HelpRow::new("space", "toggle selection"),
];

const HOME_TAB: &[HelpRow] = &[HelpRow::new("enter", "activate focused row")];

const UPDATES_TAB: &[HelpRow] = &[
    HelpRow::new("enter", "expand list / download"),
    HelpRow::new("a", "select all"),
    HelpRow::new("d", "select none"),
    HelpRow::new("r", "recheck failed"),
];

const CONFIG_TAB: &[HelpRow] = &[
    HelpRow::new("s", "save config"),
    HelpRow::new("enter (auth chip)", "log in / log out"),
];

const DOWNLOAD_TAB: &[HelpRow] = &[
    HelpRow::new("enter", "expand / collapse failed"),
    HelpRow::new("↑ / ↓", "navigate failed rows"),
    HelpRow::new("r", "retry focused failed map"),
    HelpRow::new("R", "retry all failed maps"),
    HelpRow::new("x", "close completed tab"),
];

/// Renders a centred keybindings overlay.
///
/// Call this after all other tab content and the footer have been drawn —
/// it clears the area it occupies and draws on top.
pub(crate) fn render_help_overlay(frame: &mut Frame, area: Rect) {
    let items = build_help_items();

    // Size the popup to fit all items exactly (border = 2 rows), capped at the
    // terminal height so it never overflows on very small terminals.
    let needed_height = (items.len() as u16).saturating_add(2).min(area.height);
    let [popup_area] = Layout::vertical([Constraint::Length(needed_height)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Percentage(58)])
        .flex(Flex::Center)
        .areas(popup_area);
    frame.render_widget(Clear, popup_area);

    let outer_block = modal_block(" KEYBINDINGS ");

    let inner = outer_block.inner(popup_area);
    let total = items.len();
    let visible_height = inner.height as usize;
    let (start, end) = widgets::scroll_window(&items, 0, visible_height);
    let outer_block = match widgets::scroll_indicator(start, end, total) {
        Some(span) => outer_block.title_top(Line::from(span).right_aligned()),
        None => outer_block,
    };
    frame.render_widget(outer_block, popup_area);

    frame.render_widget(List::new(items[start..end].to_vec()), inner);
}

/// Renders the pre-download "retry failed?" modal.
///
/// `enter` proceeds with retry, `n` proceeds without, `esc` cancels the
/// download. The caller (`handle_key`) intercepts all other keys.
pub(crate) fn render_retry_on_start_modal(frame: &mut Frame, area: Rect, count: usize) {
    let items = vec![
        ListItem::new(Line::from(vec![
            Span::styled(
                count.to_string(),
                Style::default().fg(accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                " failed maps from a previous run. retry them?",
                Style::default().fg(text_dim()),
            ),
        ])),
        ListItem::new(Line::from("")),
        ListItem::new(Line::from(vec![Span::styled(
            "  [enter] yes · [n] no · [esc] cancel",
            Style::default().fg(text_faint()),
        )])),
    ];
    render_fixed_popup(frame, area, 60, " RETRY FAILED? ", items);
}

/// Renders a fixed-height (5-row) centred popup with the standard modal block.
///
/// Shared by the retry-on-start and confirm-retry modals; they differ only in
/// `width_pct`, `title`, and `items`. The 5-row height (2 border + 3 content)
/// matches both modals' original layouts exactly.
fn render_fixed_popup(
    frame: &mut Frame,
    area: Rect,
    width_pct: u16,
    title: &'static str,
    items: Vec<ListItem<'static>>,
) {
    let [popup_area] = Layout::vertical([Constraint::Length(5)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Percentage(width_pct)])
        .flex(Flex::Center)
        .areas(popup_area);
    frame.render_widget(Clear, popup_area);

    let outer_block = modal_block(title);
    let inner = outer_block.inner(popup_area);
    frame.render_widget(outer_block, popup_area);

    frame.render_widget(List::new(items), inner);
}

/// Renders the "retry N failed maps?" confirmation modal.
///
/// `enter` confirms; `esc` or `q` cancels. The modal intercepts all other keys
/// while open — the caller (`handle_key`) enforces this via early return.
pub(crate) fn render_confirm_retry_modal(frame: &mut Frame, area: Rect, count: usize) {
    let items = vec![
        ListItem::new(Line::from(vec![
            Span::styled("retry ", Style::default().fg(text_dim())),
            Span::styled(
                count.to_string(),
                Style::default().fg(accent()).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" failed maps?", Style::default().fg(text_dim())),
        ])),
        ListItem::new(Line::from("")),
        ListItem::new(Line::from(vec![Span::styled(
            "  [enter] confirm  ·  [esc] cancel",
            Style::default().fg(text_faint()),
        )])),
    ];
    render_fixed_popup(frame, area, 54, " CONFIRM RETRY ", items);
}

/// Renders the config save-diff modal.
///
/// Shows each changed field as `  <label>   <old> → <new>`. If `diff` is
/// empty the popup still renders but shows "no changes" with only an esc hint.
/// `enter` confirms the save; `esc` cancels — the caller (`handle_key`)
/// enforces this via early return.
pub(crate) fn render_config_save_modal(frame: &mut Frame, area: Rect, diff: &[ConfigDiffEntry]) {
    // 2 border rows + 1 blank line + diff rows + 1 blank line + 1 hint line
    let content_rows = if diff.is_empty() {
        3u16 // "no changes to save" + blank + hint
    } else {
        (diff.len() as u16).saturating_add(4) // header + blank + rows + blank + hint
    };
    let needed_height = content_rows.saturating_add(2).min(area.height);
    let [popup_area] = Layout::vertical([Constraint::Length(needed_height)])
        .flex(Flex::Center)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Percentage(62)])
        .flex(Flex::Center)
        .areas(popup_area);
    frame.render_widget(Clear, popup_area);

    let outer_block = modal_block(MODAL_TITLE_SAVE);

    let inner = outer_block.inner(popup_area);
    frame.render_widget(outer_block, popup_area);

    let items = build_diff_items(diff);
    frame.render_widget(List::new(items), inner);
}

fn build_diff_items(diff: &[ConfigDiffEntry]) -> Vec<ListItem<'static>> {
    if diff.is_empty() {
        return vec![
            ListItem::new(Line::from(Span::styled(
                "no changes to save",
                Style::default().fg(text_dim()),
            ))),
            ListItem::new(Line::from("")),
            ListItem::new(Line::from(Span::styled(
                NO_CHANGES_HINT,
                Style::default().fg(text_faint()),
            ))),
        ];
    }

    // Compute the label column width so arrows align across all rows.
    let label_width = diff.iter().map(|e| e.label.len()).max().unwrap_or(0).max(8);

    let mut items = Vec::with_capacity(diff.len() + 4);

    items.push(ListItem::new(Line::from(Span::styled(
        "pending changes:",
        Style::default().fg(text_dim()),
    ))));
    items.push(ListItem::new(Line::from("")));

    for entry in diff {
        let pad = label_width.saturating_sub(entry.label.len());
        let mut label_cell = String::with_capacity(label_width + 4);
        label_cell.push_str("  ");
        label_cell.push_str(entry.label);
        for _ in 0..pad {
            label_cell.push(' ');
        }
        label_cell.push_str("   ");

        items.push(ListItem::new(Line::from(vec![
            Span::styled(label_cell, Style::default().fg(text_dim())),
            Span::styled(entry.old_value.clone(), Style::default().fg(text_faint())),
            Span::styled(" → ", Style::default().fg(text_faint())),
            Span::styled(
                entry.new_value.clone(),
                Style::default().fg(accent()).add_modifier(Modifier::BOLD),
            ),
        ])));
    }

    items.push(ListItem::new(Line::from("")));
    items.push(ListItem::new(Line::from(Span::styled(
        SAVE_HINT,
        Style::default().fg(text_faint()),
    ))));

    items
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
    push_section(&mut items, "download", DOWNLOAD_TAB);
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
        Style::default()
            .fg(accent_alt())
            .add_modifier(Modifier::BOLD),
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
            Style::default().fg(accent()).add_modifier(Modifier::BOLD),
        ),
        Span::styled(action, Style::default().fg(text_dim())),
    ]))
}

fn spacer() -> ListItem<'static> {
    ListItem::new(Line::from(""))
}

fn dismiss_hint() -> ListItem<'static> {
    ListItem::new(Line::from(vec![Span::styled(
        "  press ? or esc to close",
        Style::default().fg(text_faint()),
    )]))
}
