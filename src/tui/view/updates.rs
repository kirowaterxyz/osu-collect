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
                3 + form.selection.collections_state.selected().unwrap_or(0)
            } else {
                2
            }
        }
        UpdatesField::BeatmapList => {
            if form.selection.in_beatmap_list {
                let base = if form.selection.local_collections.is_empty() {
                    4
                } else {
                    5
                };
                base + form.selection.beatmaps_state.selected().unwrap_or(0)
            } else if form.selection.local_collections.is_empty() {
                3
            } else {
                4
            }
        }
    }
}

fn build_items(form: &UpdatesTab, area_height: u16) -> Vec<ListItem<'static>> {
    let mut items = Vec::new();

    items.push(client_toggle(form));
    items.push(osu_path_item(form));

    items.push(collections_header(form));

    if form.selection.in_collection_list {
        let collection_list_header_offset = 3u16;
        let collection_list_footer_offset = 3u16;
        let available_height = area_height
            .saturating_sub(collection_list_header_offset)
            .saturating_sub(collection_list_footer_offset) as usize;

        let selected_idx = form.selection.collections_state.selected().unwrap_or(0);
        let total_items = form.selection.local_collections.len();

        let scroll_offset = calculate_scroll_offset(selected_idx, total_items, available_height);

        for (i, collection) in form
            .selection
            .local_collections
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(available_height)
        {
            let is_scroll_pos = i == selected_idx;
            items.push(collection_item(collection, is_scroll_pos));
        }
    } else if !form.selection.local_collections.is_empty() {
        let selected = form.selected_collection_count();
        let total = form.selection.local_collections.len();
        items.push(ListItem::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{selected}/{total} collections selected"),
                Style::default().fg(components::TEXT_FAINT),
            ),
        ])));
    }

    items.push(beatmaps_header(form));

    if form.selection.in_beatmap_list {
        let lines_above_beatmap_list = 5u16;
        let available_height = area_height.saturating_sub(lines_above_beatmap_list) as usize;

        let selected_idx = form.selection.beatmaps_state.selected().unwrap_or(0);
        let total_items = form.selection.display_items.len();

        let scroll_offset = calculate_scroll_offset(selected_idx, total_items, available_height);

        for (i, item) in form
            .selection
            .display_items
            .iter()
            .enumerate()
            .skip(scroll_offset)
            .take(available_height)
        {
            let is_scroll_pos = i == selected_idx;
            items.push(display_item_to_list_item(item, is_scroll_pos, form));
        }
    } else if form.total_missing_count() > 0 {
        let selected = form.selected_beatmap_count();
        let total = form.total_missing_count();
        items.push(ListItem::new(Line::from(vec![
            Span::raw("    "),
            Span::styled(
                format!("{selected}/{total} beatmaps selected"),
                Style::default().fg(components::TEXT_FAINT),
            ),
        ])));
    } else {
        let is_loading = matches!(
            form.scan.scan_status,
            ScanStatus::ReadingDatabase | ScanStatus::FetchingCollection
        );
        if is_loading {
            items.push(ListItem::new(Line::from(vec![
                Span::raw("    "),
                Span::styled("loading...", Style::default().fg(components::TEXT_FAINT)),
            ])));
        }
    }

    items
}

fn calculate_scroll_offset(
    selected_idx: usize,
    total_items: usize,
    visible_height: usize,
) -> usize {
    if visible_height == 0 || total_items == 0 {
        return 0;
    }

    let half_visible = visible_height / 2;

    if selected_idx < half_visible {
        0
    } else if selected_idx >= total_items.saturating_sub(half_visible) {
        total_items.saturating_sub(visible_height)
    } else {
        selected_idx.saturating_sub(half_visible)
    }
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
                .find(|b| b.collection_id == *collection_id)
                .map(|b| b.collection_name.clone())
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

            let spans = vec![
                indent_marker(is_scroll_pos),
                Span::styled(marker, marker_style),
                Span::raw(" "),
                Span::styled(format!("#{collection_id} - {name}"), name_style),
            ];
            ListItem::new(Line::from(spans))
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

    let lazer_style = if lazer_active {
        active_style
    } else {
        inactive_style
    };
    let stable_style = if stable_active {
        active_style
    } else {
        inactive_style
    };

    let spans = vec![
        components::focus_span(focused),
        Span::styled("client: ", Style::default().fg(components::TEXT_DIM)),
        Span::styled(if lazer_active { "● " } else { "○ " }, lazer_style),
        Span::styled("Lazer", lazer_style),
        Span::styled("  ", Style::default()),
        Span::styled(if stable_active { "● " } else { "○ " }, stable_style),
        Span::styled("Stable", stable_style),
    ];

    ListItem::new(Line::from(spans))
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

    let spans = vec![
        components::focus_span(focused),
        Span::styled(
            format!("{}: ", field.label),
            Style::default().fg(components::TEXT_DIM),
        ),
        value,
    ];

    ListItem::new(Line::from(spans))
}

fn collections_header(form: &UpdatesTab) -> ListItem<'static> {
    let focused =
        form.selection.focus == UpdatesField::Collections && !form.selection.in_beatmap_list;
    let in_list = form.selection.in_collection_list;

    let prefix = components::focus_span(focused && !in_list);
    let label_style = if focused || in_list {
        Style::default().fg(components::ACCENT)
    } else {
        Style::default().fg(components::TEXT_FAINT)
    };
    let suffix = if in_list {
        "  space toggle · enter/esc exit"
    } else {
        "  space expand"
    };

    let spans = vec![
        prefix,
        Span::styled("COLLECTIONS", label_style),
        Span::styled(
            suffix.to_string(),
            Style::default().fg(components::TEXT_FAINT),
        ),
    ];

    ListItem::new(Line::from(spans))
}

fn collection_item(
    collection: &crate::app::updates::CollectionEntry,
    is_scroll_pos: bool,
) -> ListItem<'static> {
    let (marker, marker_style) = components::check_marker(collection.selected);

    let id_str = collection
        .collection_id
        .map(|id| format!("#{id}  "))
        .unwrap_or_default();

    let name_style = if is_scroll_pos {
        Style::default().fg(components::TEXT)
    } else {
        Style::default().fg(components::TEXT_MUTED)
    };

    let spans = vec![
        indent_marker(is_scroll_pos),
        Span::styled(marker, marker_style),
        Span::raw(" "),
        Span::styled(id_str, Style::default().fg(components::TEXT_FAINT)),
        Span::styled(collection.name.clone(), name_style),
        Span::styled(
            format!("  {} maps", collection.beatmap_count),
            Style::default().fg(components::TEXT_FAINT),
        ),
    ];

    ListItem::new(Line::from(spans))
}

fn beatmaps_header(form: &UpdatesTab) -> ListItem<'static> {
    let focused =
        form.selection.focus == UpdatesField::BeatmapList && !form.selection.in_collection_list;
    let in_list = form.selection.in_beatmap_list;
    let is_fetching = matches!(
        form.scan.scan_status,
        ScanStatus::ReadingDatabase | ScanStatus::FetchingCollection
    );

    let prefix = components::focus_span(focused && !in_list);
    let label_style = if focused || in_list {
        Style::default().fg(components::ACCENT)
    } else {
        Style::default().fg(components::TEXT_FAINT)
    };

    let suffix: Option<&str> = if is_fetching {
        None
    } else if in_list {
        Some("  space toggle · a all · d none · enter/esc exit")
    } else {
        Some("  space expand")
    };

    let mut spans = vec![prefix, Span::styled("MISSING BEATMAPS", label_style)];

    if let Some(text) = suffix {
        spans.push(Span::styled(
            text.to_string(),
            Style::default().fg(components::TEXT_FAINT),
        ));
    }

    ListItem::new(Line::from(spans))
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
        Span::raw("  "),
        Span::styled(
            status_text.to_string(),
            Style::default().fg(components::TEXT_FAINT),
        ),
    ];

    if beatmap.previously_deleted {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(
            "previously deleted",
            Style::default().fg(components::ACCENT_ALT),
        ));
    }

    ListItem::new(Line::from(spans))
}
