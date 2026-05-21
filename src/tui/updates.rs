use crate::app::{
    UpdatesField, UpdatesTab,
    updates::{
        BeatmapDisplayItem, BeatmapSort, CollectionEntry, CollectionSort, MissingBeatmapset,
        MissingStatus, ScanStatus,
    },
};
use crate::osu_db::OsuClient;
use crate::utils::pretty_path;
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
};

use super::widgets::{self, FOCUS_MARK, FOCUS_PAD, Metric};
use super::{accent, accent_alt, focused_label, text, text_dim, text_faint, text_muted};

const PANEL_TITLE: &str = " UPDATES ";

const SECTION_SOURCE: &str = "source";
const SECTION_COLLECTIONS: &str = "collections";
const SECTION_MISSING: &str = "missing beatmaps";

const LABEL_CLIENT: &str = "client";
const CLIENT_OPTIONS: &[&str] = &["lazer", "stable"];

const LABEL_COLLECTIONS: &str = "collections";
const LABEL_AVAILABLE: &str = "available";

const DETAIL_LOADED_SUFFIX: &str = "loaded";
const DETAIL_MISSING_SUFFIX: &str = "missing";
const DETAIL_LOADING: &str = "loading";
const DETAIL_LOCAL: &str = "local";

const METRIC_SELECTED: &str = "selected";
const METRIC_MISSING: &str = "missing";
const METRIC_FAILED: &str = "failed";

const HELP_OSU_PATH: &str = "your osu! install dir; must contain osu!.db or client.realm";

const STATUS_NOT_INSTALLED: &str = "not installed";
const TAG_PREVIOUSLY_DELETED: &str = "previously deleted";

const COUNT_SUFFIX_MAPS: &str = "maps";
const SUFFIX_SELECTED: &str = "selected";

const DIFF_PREFIX_NEW: &str = "+";
const DIFF_SUFFIX_NEW: &str = "new";
const DIFF_PREFIX_REMOVED: &str = "-";
const DIFF_SUFFIX_REMOVED: &str = "removed";
const DIFF_SEPARATOR: &str = ", ";

pub fn render(frame: &mut Frame, area: Rect, form: &UpdatesTab) {
    let block = widgets::panel_block(PANEL_TITLE);
    let inner = block.inner(area);

    let (items, focused_index) = build_items(form);
    let (start, end) = widgets::scroll_window(&items, focused_index, inner.height as usize);
    let block = match widgets::scroll_indicator(start, end, items.len()) {
        Some(span) => block.title_top(Line::from(span).right_aligned()),
        None => block,
    };
    frame.render_widget(block, area);

    let list = List::new(items[start..end].to_vec()).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn build_items(form: &UpdatesTab) -> (Vec<ListItem<'static>>, usize) {
    let mut items: Vec<ListItem<'static>> = Vec::new();
    let mut focused_index = 0usize;
    let focus = form.selection.focus;
    let in_list = form.selection.in_collection_list || form.selection.in_beatmap_list;

    items.push(widgets::section_header(SECTION_SOURCE));
    if focus == UpdatesField::ClientType && !in_list {
        focused_index = items.len();
    }
    items.push(widgets::cycle_item(
        LABEL_CLIENT,
        CLIENT_OPTIONS,
        client_label(form.path.client_type),
        focus == UpdatesField::ClientType && !in_list,
    ));
    if focus == UpdatesField::OsuPath && !in_list {
        focused_index = items.len();
    }
    items.push(osu_path_item(form));
    if focus == UpdatesField::OsuPath && !in_list {
        items.push(widgets::help_item(HELP_OSU_PATH));
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_COLLECTIONS));
    if focus == UpdatesField::Collections && !in_list {
        focused_index = items.len();
    }
    items.push(collections_header(form));
    if form.selection.in_collection_list {
        let selected_idx = form.selection.collections_state.unwrap_or(0);
        for (i, collection) in form.selection.local_collections.iter().enumerate() {
            let is_sel = i == selected_idx;
            if is_sel && focus == UpdatesField::Collections {
                focused_index = items.len();
            }
            let counts = collection
                .collection_id
                .map(|cid| count_selected(&form.selection.cached_missing_sets, cid));
            items.push(collection_item(collection, is_sel, counts));
        }
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_MISSING));
    if focus == UpdatesField::BeatmapList && !in_list {
        focused_index = items.len();
    }
    items.push(beatmaps_header(form));
    if form.selection.in_beatmap_list {
        let selected_idx = form.selection.beatmaps_state.unwrap_or(0);
        for (i, item) in form.selection.display_items.iter().enumerate() {
            let is_sel = i == selected_idx;
            if is_sel && focus == UpdatesField::BeatmapList {
                focused_index = items.len();
            }
            items.push(display_item(item, is_sel, form));
        }
    }
    items.push(widgets::spacer());

    items.push(summary_metrics(form));
    (items, focused_index)
}

fn summary_metrics(form: &UpdatesTab) -> ListItem<'static> {
    let mut metrics = vec![
        Metric::accent(METRIC_SELECTED, form.selected_beatmap_count().to_string()),
        Metric::muted(METRIC_MISSING, form.total_missing_count().to_string()),
    ];
    if form.scan.failed_beatmapset_count > 0 {
        metrics.push(Metric::muted(
            METRIC_FAILED,
            form.scan.failed_beatmapset_count.to_string(),
        ));
    }
    widgets::summary_item(&metrics)
}

fn display_item(
    item: &BeatmapDisplayItem,
    is_scroll_pos: bool,
    form: &UpdatesTab,
) -> ListItem<'static> {
    match item {
        BeatmapDisplayItem::CollectionHeader { collection_id } => {
            let name = form
                .selection
                .cached_missing_sets
                .iter()
                .find(|beatmap| beatmap.collection_id == *collection_id)
                .map(|beatmap| beatmap.collection_name.clone())
                .unwrap_or_default();

            let visible_cache_indices: Vec<usize> = form
                .selection
                .visible_missing
                .iter()
                .copied()
                .filter(|&cache_idx| {
                    form.selection
                        .cached_missing_sets
                        .get(cache_idx)
                        .map(|beatmap| beatmap.collection_id == *collection_id)
                        .unwrap_or(false)
                })
                .collect();

            let all_selected = !visible_cache_indices.is_empty()
                && visible_cache_indices.iter().all(|&cache_idx| {
                    form.selection
                        .cached_missing_sets
                        .get(cache_idx)
                        .map(|beatmap| beatmap.selected)
                        .unwrap_or(false)
                });
            let (marker, marker_style) = widgets::check_marker(all_selected);

            ListItem::new(Line::from(vec![
                indent_marker(is_scroll_pos),
                Span::styled(marker, marker_style),
                Span::styled(
                    format!(" #{collection_id}"),
                    Style::default().fg(text_faint()),
                ),
                Span::styled(
                    format!("  {name}"),
                    Style::default()
                        .fg(accent_alt())
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        }
        BeatmapDisplayItem::Beatmap { cache_index } => form
            .selection
            .cached_missing_sets
            .get(*cache_index)
            .map(|beatmap| beatmap_item(beatmap, is_scroll_pos))
            .unwrap_or_else(|| ListItem::new(Line::from(""))),
    }
}

fn indent_marker(is_scroll_pos: bool) -> Span<'static> {
    if is_scroll_pos {
        Span::styled(FOCUS_MARK, Style::default().fg(accent()))
    } else {
        Span::raw(FOCUS_PAD)
    }
}

fn client_label(client: OsuClient) -> &'static str {
    match client {
        OsuClient::Lazer => "lazer",
        OsuClient::Stable => "stable",
    }
}

fn osu_path_item(form: &UpdatesTab) -> ListItem<'static> {
    let focused = form.selection.focus == UpdatesField::OsuPath
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list;
    let field = &form.path.osu_path;

    // When focused and the user is actively typing, show the raw value so
    // they can see and edit exactly what they typed. When not focused,
    // collapse the home prefix to `~` for readability.
    let display_value = if focused || field.value.is_empty() {
        field.value.clone()
    } else {
        pretty_path(&field.value).into_owned()
    };

    let value = if field.value.is_empty() {
        Span::styled(
            pretty_path(&field.placeholder).into_owned(),
            Style::default().fg(text_faint()),
        )
    } else if form.is_path_auto_detected() {
        Span::styled(display_value, Style::default().fg(text_faint()))
    } else {
        Span::styled(display_value, Style::default().fg(accent()))
    };

    ListItem::new(Line::from(vec![
        widgets::focus_span(focused),
        Span::styled(
            format!("{}: ", field.label.to_lowercase()),
            focused_label(focused),
        ),
        value,
    ]))
}

fn collections_header(form: &UpdatesTab) -> ListItem<'static> {
    let focused =
        form.selection.focus == UpdatesField::Collections && !form.selection.in_beatmap_list;
    let sort = form.selection.collection_sort;
    let detail = if sort == CollectionSort::Default {
        format!(
            "{} {DETAIL_LOADED_SUFFIX}",
            form.selection.local_collections.len()
        )
    } else {
        format!(
            "{} {DETAIL_LOADED_SUFFIX}  · {}",
            form.selection.local_collections.len(),
            sort.label(),
        )
    };
    widgets::disclosure_row(
        LABEL_COLLECTIONS,
        detail,
        form.selection.in_collection_list,
        focused,
    )
}

fn collection_item(
    collection: &CollectionEntry,
    is_scroll_pos: bool,
    missing_counts: Option<(usize, usize)>,
) -> ListItem<'static> {
    let (marker, marker_style) = widgets::check_marker(collection.selected);
    let id = collection
        .collection_id
        .map(|id| format!("#{id}"))
        .unwrap_or_else(|| DETAIL_LOCAL.to_string());
    let name_style = if is_scroll_pos {
        Style::default().fg(text())
    } else {
        Style::default().fg(text_muted())
    };

    let mut spans = vec![
        indent_marker(is_scroll_pos),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {id}"), Style::default().fg(text_faint())),
        Span::styled(format!("  {}", collection.name), name_style),
        Span::styled(
            format!("  {} {COUNT_SUFFIX_MAPS}", collection.beatmap_count),
            Style::default().fg(text_faint()),
        ),
    ];

    if let Some((n, total)) = missing_counts {
        spans.push(Span::styled(
            format!("  [{n}/{total} {SUFFIX_SELECTED}]"),
            Style::default().fg(text_faint()),
        ));
    }

    let new_count = missing_counts.map(|(_, total)| total).unwrap_or(0);
    let removed_count = collection.removed_count;
    if new_count > 0 || removed_count > 0 {
        spans.push(Span::raw("  "));
        let mut diff_parts = Vec::with_capacity(2);
        if new_count > 0 {
            diff_parts.push(Span::styled(
                format!("{DIFF_PREFIX_NEW}{new_count} {DIFF_SUFFIX_NEW}"),
                Style::default().fg(accent()),
            ));
        }
        if removed_count > 0 {
            if new_count > 0 {
                diff_parts.push(Span::raw(DIFF_SEPARATOR));
            }
            diff_parts.push(Span::styled(
                format!("{DIFF_PREFIX_REMOVED}{removed_count} {DIFF_SUFFIX_REMOVED}"),
                Style::default().fg(text_muted()),
            ));
        }
        spans.extend(diff_parts);
    }

    ListItem::new(Line::from(spans))
}

/// Returns `(n_selected, total)` for `cached` entries belonging to `collection_id`.
pub(super) fn count_selected(cached: &[MissingBeatmapset], collection_id: u64) -> (usize, usize) {
    let mut total = 0usize;
    let mut selected = 0usize;
    for beatmap in cached {
        if beatmap.collection_id as u64 == collection_id {
            total += 1;
            if beatmap.selected {
                selected += 1;
            }
        }
    }
    (selected, total)
}

fn beatmaps_header(form: &UpdatesTab) -> ListItem<'static> {
    let focused = form.selection.focus == UpdatesField::BeatmapList
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list;
    let sort = form.selection.beatmap_sort;
    let detail = if is_scanning(form) {
        DETAIL_LOADING.to_string()
    } else if sort == BeatmapSort::Default {
        format!("{} {DETAIL_MISSING_SUFFIX}", form.total_missing_count())
    } else {
        format!(
            "{} {DETAIL_MISSING_SUFFIX}  · {}",
            form.total_missing_count(),
            sort.label(),
        )
    };

    widgets::disclosure_row(
        LABEL_AVAILABLE,
        detail,
        form.selection.in_beatmap_list,
        focused,
    )
}

fn beatmap_item(beatmap: &MissingBeatmapset, is_scroll_pos: bool) -> ListItem<'static> {
    let (marker, marker_style) = widgets::check_marker(beatmap.selected);
    let status_text = match beatmap.status {
        MissingStatus::NotInstalled => STATUS_NOT_INSTALLED,
    };

    let mut spans = vec![
        indent_marker(is_scroll_pos),
        Span::styled(marker, marker_style),
        Span::styled(format!(" #{}", beatmap.id), Style::default().fg(text_dim())),
        Span::styled(
            format!("  {status_text}"),
            Style::default().fg(text_faint()),
        ),
    ];

    if beatmap.previously_deleted {
        spans.push(Span::styled(
            format!("  {TAG_PREVIOUSLY_DELETED}"),
            Style::default().fg(accent_alt()),
        ));
    }

    ListItem::new(Line::from(spans))
}

fn is_scanning(form: &UpdatesTab) -> bool {
    matches!(
        form.scan.scan_status,
        ScanStatus::ReadingDatabase
            | ScanStatus::FetchingCollection
            | ScanStatus::CheckingFailedMaps
    )
}

#[cfg(test)]
#[path = "../../tests/unit/tui_updates.rs"]
mod tests;
