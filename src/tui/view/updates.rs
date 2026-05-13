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

    let items = build_items(form, inner.height);
    let focused_index = focused_item_index(form);
    let visible_height = inner.height as usize;
    let (start, end) = components::scroll_window(&items, focused_index, visible_height);
    let visible_items = items[start..end].to_vec();

    let list = List::new(visible_items).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn focused_item_index(form: &UpdatesTab) -> usize {
    match form.selection.focus {
        UpdatesField::ClientType => 0,
        UpdatesField::OsuPath => 1,
        UpdatesField::Collections => {
            if form.selection.in_collection_list {
                4 + form.selection.collections_state.selected().unwrap_or(0)
            } else {
                3
            }
        }
        UpdatesField::BeatmapList => {
            if form.selection.in_beatmap_list {
                let base = if form.selection.local_collections.is_empty() {
                    5
                } else {
                    6
                };
                base + form.selection.beatmaps_state.selected().unwrap_or(0)
            } else if form.selection.local_collections.is_empty() {
                4
            } else {
                5
            }
        }
        UpdatesField::RecheckFailedMaps => form.selection.display_items.len() + 6,
    }
}

fn build_items(form: &UpdatesTab, area_height: u16) -> Vec<ListItem<'static>> {
    let mut items = vec![
        client_toggle(form),
        osu_path_item(form),
        components::help_item("uses home download settings: mirrors, threads, retries, videos"),
        collections_header(form),
    ];

    if form.selection.in_collection_list {
        let selected_idx = form.selection.collections_state.selected().unwrap_or(0);
        let (start, end) = components::scroll_window(
            &form.selection.local_collections,
            selected_idx,
            area_height.saturating_sub(6) as usize,
        );

        for (index, collection) in form.selection.local_collections[start..end]
            .iter()
            .enumerate()
        {
            items.push(collection_item(collection, start + index == selected_idx));
        }
    } else if !form.selection.local_collections.is_empty() {
        items.push(components::summary_item(&[
            components::Metric::accent("selected", form.selected_collection_count().to_string()),
            components::Metric::muted("total", form.selection.local_collections.len().to_string()),
        ]));
    }

    items.push(beatmaps_header(form));

    if form.selection.in_beatmap_list {
        let selected_idx = form.selection.beatmaps_state.selected().unwrap_or(0);
        let (start, end) = components::scroll_window(
            &form.selection.display_items,
            selected_idx,
            area_height.saturating_sub(5) as usize,
        );

        for (index, item) in form.selection.display_items[start..end].iter().enumerate() {
            items.push(display_item_to_list_item(
                item,
                start + index == selected_idx,
                form,
            ));
        }
    } else if form.total_missing_count() > 0 {
        items.push(components::summary_item(&[
            components::Metric::accent("selected", form.selected_beatmap_count().to_string()),
            components::Metric::muted("missing", form.total_missing_count().to_string()),
        ]));
    } else if is_scanning(form) {
        items.push(components::summary_item(&[components::Metric::muted(
            "status", "loading",
        )]));
    }

    items.push(recheck_failed_item(form));
    items
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
    let detail = if form.selection.in_collection_list {
        format!("{count} loaded · enter closes")
    } else {
        format!("{count} loaded · space expands")
    };

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
    } else if form.selection.in_beatmap_list {
        "space toggles · a all · d none".to_string()
    } else {
        format!("{} missing · space expands", form.total_missing_count())
    };

    components::disclosure_row(
        "missing beatmaps",
        detail,
        form.selection.in_beatmap_list,
        focused,
    )
}

fn recheck_failed_item(form: &UpdatesTab) -> ListItem<'static> {
    let focused = form.selection.focus == UpdatesField::RecheckFailedMaps
        && !form.selection.in_collection_list
        && !form.selection.in_beatmap_list;
    let count = form.scan.failed_beatmapset_count;
    let detail = if count == 0 {
        "no failed maps".to_string()
    } else if form.can_recheck_failed_maps() {
        format!("{count} hidden · space rechecks")
    } else {
        format!("{count} hidden · busy")
    };

    components::disclosure_row("failed maps", detail, false, focused)
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
        ScanStatus::ReadingDatabase | ScanStatus::FetchingCollection | ScanStatus::CheckingFailedMaps
    )
}

fn marker_style(active: bool) -> Style {
    if active {
        Style::default().fg(components::ACCENT)
    } else {
        Style::default().fg(components::TEXT_FAINT)
    }
}
