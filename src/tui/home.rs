use crate::app::runtime::ProbeResult;
use crate::app::url_history::UrlHistoryEntry;
use crate::app::{Banner, HomeField, HomeTab, ResolveState};
use ratatui::{
    Frame,
    layout::{Constraint, Flex, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, List, ListItem, Padding},
};

use super::banner::{banner_height, render_banners};
use super::widgets::{self, Metric};
use super::{
    HELP_CUSTOM_MIRROR, accent, accent_alt, bg_raised, danger, line, mirror_label, success,
    text_dim, text_faint, text_muted,
};
use osu_downloader::MirrorKind;

const PANEL_TITLE: &str = " HOME ";

const SECTION_COLLECTION: &str = "collection";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_DOWNLOAD: &str = "download";

/// Minimum content-area height before switching to compact layout.
const COMPACT_HEIGHT: u16 = 12;

const LABEL_OVERWRITE: &str = "overwrite existing";
const LABEL_NO_VIDEO: &str = "no video";

const METRIC_THREADS: &str = "threads";
const METRIC_MIRRORS: &str = "mirrors";

const LABEL_START_DOWNLOAD: &str = "start download";

/// Returns the terminal caret position when a text field is focused, for the
/// caller to apply. `None` when no caret should show (non-text focus, or the
/// history dropdown is overlaying the collection row).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    form: &HomeTab,
    banners: &[Banner],
) -> Option<(u16, u16)> {
    if area.height < COMPACT_HEIGHT {
        return render_compact(frame, area, form);
    }

    let (banner_area, content_area) = split_banner_area(area, banners);
    render_banners(frame, banner_area, banners);
    let cursor = render_content(frame, content_area, form);

    if form.dropdown_open && !form.url_history.entries.is_empty() {
        render_url_dropdown(frame, content_area, form);
        // dropdown overlays the collection row — suppress the caret
        return None;
    }
    cursor
}

/// Compact render: all focusable fields without section headers, spacers, or help lines.
///
/// Navigation is identical to normal mode — the full `HOME_FIELDS` cycle still applies.
/// Only decorative chrome is stripped to reclaim vertical space.
fn render_compact(frame: &mut Frame, area: Rect, form: &HomeTab) -> Option<(u16, u16)> {
    let focus = form.focus;
    let mut items = widgets::FormItems::new(focus);

    items.push_focusable(
        HomeField::Collection,
        widgets::input_item(&form.collection, focus == HomeField::Collection),
    );
    if let Some((state, text)) = &form.collection_resolve {
        items.push(resolve_row(*state, text));
    }
    items.push_focusable(
        HomeField::Directory,
        widgets::input_item(&form.directory, focus == HomeField::Directory),
    );

    items.push_focusable(
        HomeField::CustomMirror,
        widgets::input_item(&form.custom_mirror, focus == HomeField::CustomMirror),
    );

    push_mirror_rows(&mut items, form, focus);

    items.push_focusable(
        HomeField::Threads,
        widgets::stepper_item(
            form.threads.label,
            form.resolved_threads(),
            form.default_threads,
            focus == HomeField::Threads,
        ),
    );
    items.push_focusable(
        HomeField::AutoOverwrite,
        widgets::row_item(
            LABEL_OVERWRITE,
            None,
            form.auto_overwrite,
            focus == HomeField::AutoOverwrite,
        ),
    );
    items.push_focusable(
        HomeField::NoVideo,
        widgets::row_item(
            LABEL_NO_VIDEO,
            None,
            form.no_video,
            focus == HomeField::NoVideo,
        ),
    );

    items.push(summary_item(form));
    items.push_focusable(
        HomeField::Download,
        widgets::button_item(
            LABEL_START_DOWNLOAD,
            focus == HomeField::Download,
            can_download(form),
        ),
    );

    let cursor_col = form.focused_input().map(widgets::input_cursor_col);
    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index, cursor_col)
}

/// Whether the form has the minimum inputs a download needs: a collection
/// reference and at least one enabled mirror. Drives the button's enabled state;
/// final validation still happens in `HomeTab::build_request` on activation.
fn can_download(form: &HomeTab) -> bool {
    !form.collection.value.trim().is_empty() && form.mirror_count() > 0
}

fn render_content(frame: &mut Frame, area: Rect, form: &HomeTab) -> Option<(u16, u16)> {
    let focus = form.focus;
    let mut items = widgets::FormItems::new(focus);

    items.push(widgets::section_header(SECTION_COLLECTION));
    items.push_focusable(
        HomeField::Collection,
        widgets::input_item(&form.collection, focus == HomeField::Collection),
    );
    if let Some((state, text)) = &form.collection_resolve {
        items.push(resolve_row(*state, text));
    }
    items.push_focusable(
        HomeField::Directory,
        widgets::input_item(&form.directory, focus == HomeField::Directory),
    );
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_MIRRORS));
    items.push_focusable(
        HomeField::CustomMirror,
        widgets::input_item(&form.custom_mirror, focus == HomeField::CustomMirror),
    );
    if focus == HomeField::CustomMirror {
        items.push(widgets::help_item(HELP_CUSTOM_MIRROR));
    }

    push_mirror_rows(&mut items, form, focus);
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_DOWNLOAD));
    items.push_focusable(
        HomeField::Threads,
        widgets::stepper_item(
            form.threads.label,
            form.resolved_threads(),
            form.default_threads,
            focus == HomeField::Threads,
        ),
    );
    items.push_focusable(
        HomeField::AutoOverwrite,
        widgets::row_item(
            LABEL_OVERWRITE,
            None,
            form.auto_overwrite,
            focus == HomeField::AutoOverwrite,
        ),
    );
    items.push_focusable(
        HomeField::NoVideo,
        widgets::row_item(
            LABEL_NO_VIDEO,
            None,
            form.no_video,
            focus == HomeField::NoVideo,
        ),
    );
    items.push(widgets::spacer());

    items.push(summary_item(form));
    items.push(widgets::spacer());
    items.push_focusable(
        HomeField::Download,
        widgets::button_item(
            LABEL_START_DOWNLOAD,
            focus == HomeField::Download,
            can_download(form),
        ),
    );

    let cursor_col = form.focused_input().map(widgets::input_cursor_col);
    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index, cursor_col)
}

/// Pushes the four built-in mirror toggle rows, each with its latency suffix.
///
/// Shared by `render_compact` and `render_content` — the row content is
/// identical in both paths; only the surrounding chrome differs.
fn push_mirror_rows(items: &mut widgets::FormItems<HomeField>, form: &HomeTab, focus: HomeField) {
    let mirror_states = [
        (HomeField::MirrorOsuDirect, form.osu_direct),
        (HomeField::MirrorNerinyan, form.nerinyan),
        (HomeField::MirrorSayobot, form.sayobot),
        (HomeField::MirrorNekoha, form.nekoha),
    ];
    for (kind, (field, on)) in MirrorKind::BUILTINS.iter().zip(mirror_states) {
        let latency = form.mirror_latency.get(kind).copied();
        items.push_focusable(
            field,
            mirror_row_item(
                mirror_label(*kind),
                kind.host(),
                on,
                focus == field,
                latency,
            ),
        );
    }
}

/// The threads / mirrors summary row, shared by both render paths.
fn summary_item(form: &HomeTab) -> ListItem<'static> {
    widgets::summary_item(&[
        Metric::accent(METRIC_THREADS, form.resolved_threads().to_string()),
        Metric::accent(METRIC_MIRRORS, form.mirror_count().to_string()),
    ])
}

/// Mirror toggle row with an optional latency suffix.
///
/// `latency` mirrors `HomeTab::mirror_latency` semantics:
/// - `None`         → not yet probed (no suffix)
/// - `Some(None)`   → probe in flight (`…`)
/// - `Some(Some(_))` → result received
fn mirror_row_item(
    label: &str,
    host: &str,
    on: bool,
    focused: bool,
    latency: Option<Option<ProbeResult>>,
) -> ListItem<'static> {
    use super::focused_label;
    use widgets::{check_marker, focus_span};

    let (marker, marker_style) = check_marker(on);
    let mut spans = vec![
        focus_span(focused),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {label}"), focused_label(focused)),
        Span::styled(format!("  {host}"), Style::default().fg(text_faint())),
    ];

    match latency {
        None => {}
        Some(None) => {
            spans.push(Span::styled("  …", Style::default().fg(text_dim())));
        }
        Some(Some(ProbeResult::Ms(ms))) => {
            let mut s = String::with_capacity(10);
            s.push_str("  ✓ ");
            s.push_str(&ms.to_string());
            s.push_str("ms");
            spans.push(Span::styled(s, Style::default().fg(success())));
        }
        Some(Some(ProbeResult::Timeout)) => {
            spans.push(Span::styled("  ✗timeout", Style::default().fg(danger())));
        }
        Some(Some(ProbeResult::Error)) => {
            spans.push(Span::styled("  ✗N/A", Style::default().fg(danger())));
        }
    }

    ListItem::new(Line::from(spans))
}

/// Max visible rows in the dropdown before scroll indicator kicks in.
const DROPDOWN_MAX_VISIBLE: usize = 5;

/// Renders the URL history dropdown anchored below the collection field.
///
/// The dropdown floats as an overlay inside `area`. It is sized to fit up to
/// `DROPDOWN_MAX_VISIBLE` entries plus a border (2 rows), and is
/// horizontally shrunk to 80 % of the panel width.
fn render_url_dropdown(frame: &mut Frame, area: Rect, form: &HomeTab) {
    let entries = &form.url_history.entries;
    let visible = DROPDOWN_MAX_VISIBLE.min(entries.len());
    // +2 for top/bottom borders
    let popup_height = (visible as u16).saturating_add(2).min(area.height);

    // Place the popup near the top of the area (just below the field rows).
    // Use Flex::Start so the popup anchors at the top of the available space.
    let [popup_area] = Layout::vertical([Constraint::Length(popup_height)])
        .flex(Flex::Start)
        .areas(area);
    let [popup_area] = Layout::horizontal([Constraint::Percentage(80)])
        .flex(Flex::Start)
        .areas(popup_area);

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Plain)
        .border_style(Style::default().fg(line()))
        .style(Style::default().bg(bg_raised()))
        .title(Span::styled(" history ", Style::default().fg(accent_alt())))
        .padding(Padding::new(1, 1, 0, 0));

    let inner = block.inner(popup_area);
    let total = entries.len();
    let selected = form.dropdown_selected.unwrap_or(0);
    let (start, end) = widgets::scroll_window(entries.as_slice(), selected, inner.height as usize);

    let block = match widgets::scroll_indicator(start, end, total) {
        Some(span) => block.title_top(Line::from(span).right_aligned()),
        None => block,
    };
    frame.render_widget(block, popup_area);

    let items: Vec<ListItem<'static>> = entries[start..end]
        .iter()
        .enumerate()
        .map(|(rel_idx, entry)| {
            let abs_idx = start + rel_idx;
            dropdown_entry_item(entry, abs_idx == selected)
        })
        .collect();

    frame.render_widget(List::new(items), inner);
}

fn dropdown_entry_item(entry: &UrlHistoryEntry, selected: bool) -> ListItem<'static> {
    // Single-line: "Name (count) — url"
    let label_style = if selected {
        Style::default().fg(accent()).add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(text_dim())
    };
    let url_style = Style::default().fg(text_faint());

    let mut name_part = entry.name.clone();
    name_part.push_str(" (");
    name_part.push_str(&entry.count.to_string());
    name_part.push(')');

    let mut url_part = String::with_capacity(4 + entry.url.len());
    url_part.push_str(" — ");
    url_part.push_str(&entry.url);

    ListItem::new(Line::from(vec![
        Span::styled(name_part, label_style),
        Span::styled(url_part, url_style),
    ]))
}

const RESOLVE_PREFIX: &str = "  └ ";
const RESOLVE_ARROW: &str = "→ ";

fn resolve_row(state: ResolveState, text: &str) -> ListItem<'static> {
    let (arrow_color, text_color) = match state {
        ResolveState::Loading => (text_muted(), text_faint()),
        ResolveState::Success => (success(), text_faint()),
        ResolveState::Error => (danger(), danger()),
    };
    ListItem::new(Line::from(vec![
        Span::styled(RESOLVE_PREFIX, Style::default().fg(text_faint())),
        Span::styled(RESOLVE_ARROW, Style::default().fg(arrow_color)),
        Span::styled(text.to_string(), Style::default().fg(text_color)),
    ]))
}

/// Split `area` into a banner strip (top) and the main content area (rest).
///
/// When `banners` is empty the banner strip has height 0 and `content_area`
/// is the full `area`. Only inserts rows for the actual number of banners so
/// the content area is never unnecessarily shrunk.
fn split_banner_area(area: Rect, banners: &[Banner]) -> (Rect, Rect) {
    let n = banner_height(banners);
    if n == 0 {
        return (Rect { height: 0, ..area }, area);
    }
    let banner_height_clamped = n.min(area.height);
    let content_height = area.height.saturating_sub(banner_height_clamped);
    let banner_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: banner_height_clamped,
    };
    let content_area = Rect {
        x: area.x,
        y: area.y + banner_height_clamped,
        width: area.width,
        height: content_height,
    };
    (banner_area, content_area)
}
