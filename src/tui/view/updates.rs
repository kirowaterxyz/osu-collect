use crate::app::{
    UpdatesField, UpdatesTab,
    updates::{BeatmapDisplayItem, ScanStatus},
};
use ratatui::{
    Frame,
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{List, ListItem},
};

use super::{UpdatesView, components};

pub fn render(frame: &mut Frame, area: Rect, view: UpdatesView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &UpdatesTab) {
    let block = components::panel_block("updates");
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let built = build_view(form, inner.height);
    let visible_height = inner.height as usize;
    let (start, end) = components::scroll_window(&built.items, built.focused_index, visible_height);
    let visible_items = built.items[start..end].to_vec();

    let list = List::new(visible_items).highlight_symbol("");
    frame.render_widget(list, inner);
}

struct BuiltView {
    items: Vec<ListItem<'static>>,
    focused_index: usize,
}

fn build_view(form: &UpdatesTab, area_height: u16) -> BuiltView {
    let mut items: Vec<ListItem<'static>> = Vec::new();
    let mut focused_index: usize = 0;
    let focus = form.selection.focus;

    // SOURCE
    items.push(components::section_header("source"));
    if focus == UpdatesField::ClientType
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list
    {
        focused_index = items.len();
    }
    items.push(client_toggle(form));
    if focus == UpdatesField::OsuPath
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list
    {
        focused_index = items.len();
    }
    items.push(osu_path_item(form));
    items.push(components::spacer());

    // COLLECTIONS
    items.push(components::section_header("collections"));
    if focus == UpdatesField::Collections && !form.selection.in_collection_list {
        focused_index = items.len();
    }
    items.push(collections_header(form));
    if form.selection.in_collection_list {
        let selected_idx = form.selection.collections_state.selected().unwrap_or(0);
        let (start, end) = components::scroll_window(
            &form.selection.local_collections,
            selected_idx,
            area_height.saturating_sub(10) as usize,
        );
        for (i, collection) in form.selection.local_collections[start..end]
            .iter()
            .enumerate()
        {
            let actual = start + i;
            let is_sel = actual == selected_idx;
            if is_sel && focus == UpdatesField::Collections {
                focused_index = items.len();
            }
            items.push(collection_item(collection, is_sel));
        }
    }
    items.push(components::spacer());

    // MISSING BEATMAPS
    items.push(components::section_header("missing beatmaps"));
    if focus == UpdatesField::BeatmapList && !form.selection.in_beatmap_list {
        focused_index = items.len();
    }
    items.push(beatmaps_header(form));
    if form.selection.in_beatmap_list {
        let selected_idx = form.selection.beatmaps_state.selected().unwrap_or(0);
        let (start, end) = components::scroll_window(
            &form.selection.display_items,
            selected_idx,
            area_height.saturating_sub(10) as usize,
        );
        for (i, item) in form.selection.display_items[start..end].iter().enumerate() {
            let actual = start + i;
            let is_sel = actual == selected_idx;
            if is_sel && focus == UpdatesField::BeatmapList {
                focused_index = items.len();
            }
            items.push(display_item_to_list_item(item, is_sel, form));
        }
    }
    items.push(components::spacer());

    // SUMMARY
    items.push(summary_metrics(form));

    BuiltView {
        items,
        focused_index,
    }
}

fn summary_metrics(form: &UpdatesTab) -> ListItem<'static> {
    let mut metrics = vec![
        components::Metric::accent("selected", form.selected_beatmap_count().to_string()),
        components::Metric::muted("missing", form.total_missing_count().to_string()),
    ];
    if form.scan.failed_beatmapset_count > 0 {
        metrics.push(components::Metric::muted(
            "failed",
            form.scan.failed_beatmapset_count.to_string(),
        ));
    }
    components::summary_item(&metrics)
}

fn display_item_to_list_item(
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
            let (marker, marker_style) = components::check_marker(all_selected);

            let name_style = Style::default()
                .fg(components::ACCENT_ALT)
                .add_modifier(Modifier::BOLD);

            ListItem::new(Line::from(vec![
                indent_marker(is_scroll_pos),
                Span::styled(marker, marker_style),
                Span::styled(
                    format!(" #{collection_id}"),
                    Style::default().fg(components::TEXT_FAINT),
                ),
                Span::styled(format!("  {name}"), name_style),
            ]))
        }
        BeatmapDisplayItem::Beatmap { cache_index } => {
            if let Some(beatmap) = form.selection.cached_missing_sets.get(*cache_index) {
                beatmap_item(beatmap, is_scroll_pos)
            } else {
                ListItem::new(Line::from(""))
            }
        }
    }
}

fn indent_marker(is_scroll_pos: bool) -> Span<'static> {
    if is_scroll_pos {
        Span::styled(
            components::FOCUS_MARK,
            Style::default().fg(components::ACCENT),
        )
    } else {
        Span::raw(components::FOCUS_PAD)
    }
}

fn client_toggle(form: &UpdatesTab) -> ListItem<'static> {
    let focused = form.selection.focus == UpdatesField::ClientType
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list;

    let active_style = Style::default().fg(components::ACCENT);
    let inactive_style = Style::default().fg(components::TEXT_FAINT);
    let lazer_active = form.path.client_type == crate::osu_db::OsuClient::Lazer;
    let stable_active = form.path.client_type == crate::osu_db::OsuClient::Stable;

    ListItem::new(Line::from(vec![
        components::focus_span(focused),
        Span::styled("client: ", Style::default().fg(components::TEXT_DIM)),
        Span::styled(
            if lazer_active { "◉ " } else { "○ " },
            marker_style(lazer_active),
        ),
        Span::styled(
            "lazer",
            if lazer_active {
                active_style
            } else {
                inactive_style
            },
        ),
        Span::styled("  ", Style::default()),
        Span::styled(
            if stable_active { "◉ " } else { "○ " },
            marker_style(stable_active),
        ),
        Span::styled(
            "stable",
            if stable_active {
                active_style
            } else {
                inactive_style
            },
        ),
    ]))
}

fn osu_path_item(form: &UpdatesTab) -> ListItem<'static> {
    let focused = form.selection.focus == UpdatesField::OsuPath
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list;
    let field = &form.path.osu_path;

    let value = if field.value.is_empty() {
        Span::styled(
            field.placeholder.clone(),
            Style::default().fg(components::TEXT_FAINT),
        )
    } else if form.is_path_auto_detected() {
        Span::styled(
            field.value.clone(),
            Style::default().fg(components::TEXT_FAINT),
        )
    } else {
        Span::styled(field.value.clone(), Style::default().fg(components::ACCENT))
    };

    ListItem::new(Line::from(vec![
        components::focus_span(focused),
        Span::styled(
            format!("{}: ", field.label.to_lowercase()),
            Style::default().fg(components::TEXT_DIM),
        ),
        value,
    ]))
}

fn collections_header(form: &UpdatesTab) -> ListItem<'static> {
    let focused =
        form.selection.focus == UpdatesField::Collections && !form.selection.in_beatmap_list;
    let count = form.selection.local_collections.len();
    let detail = format!("{count} loaded");

    components::disclosure_row(
        "collections",
        detail,
        form.selection.in_collection_list,
        focused,
    )
}

fn collection_item(
    collection: &crate::app::updates::CollectionEntry,
    is_scroll_pos: bool,
) -> ListItem<'static> {
    let (marker, marker_style) = components::check_marker(collection.selected);
    let id = collection
        .collection_id
        .map(|id| format!("#{id}"))
        .unwrap_or_else(|| "local".to_string());
    let name_style = if is_scroll_pos {
        Style::default().fg(components::TEXT)
    } else {
        Style::default().fg(components::TEXT_MUTED)
    };

    ListItem::new(Line::from(vec![
        indent_marker(is_scroll_pos),
        Span::styled(marker, marker_style),
        Span::styled(
            format!(" {id}"),
            Style::default().fg(components::TEXT_FAINT),
        ),
        Span::styled(format!("  {}", collection.name), name_style),
        Span::styled(
            format!("  {} maps", collection.beatmap_count),
            Style::default().fg(components::TEXT_FAINT),
        ),
    ]))
}

fn beatmaps_header(form: &UpdatesTab) -> ListItem<'static> {
    let focused = form.selection.focus == UpdatesField::BeatmapList
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list;
    let detail = if is_scanning(form) {
        "loading".to_string()
    } else {
        format!("{} missing", form.total_missing_count())
    };

    components::disclosure_row("available", detail, form.selection.in_beatmap_list, focused)
}

fn beatmap_item(
    beatmap: &crate::app::updates::MissingBeatmapset,
    is_scroll_pos: bool,
) -> ListItem<'static> {
    let (marker, marker_style) = components::check_marker(beatmap.selected);
    let status_text = match beatmap.status {
        crate::app::updates::MissingStatus::NotInstalled => "not installed",
    };

    let mut spans = vec![
        indent_marker(is_scroll_pos),
        Span::styled(marker, marker_style),
        Span::styled(
            format!(" #{}", beatmap.id),
            Style::default().fg(components::TEXT_DIM),
        ),
        Span::styled(
            format!("  {status_text}"),
            Style::default().fg(components::TEXT_FAINT),
        ),
    ];

    if beatmap.previously_deleted {
        spans.push(Span::styled(
            "  previously deleted",
            Style::default().fg(components::ACCENT_ALT),
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

fn marker_style(active: bool) -> Style {
    if active {
        Style::default().fg(components::ACCENT)
    } else {
        Style::default().fg(components::TEXT_FAINT)
    }
}
