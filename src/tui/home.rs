use crate::app::runtime::ProbeResult;
use crate::app::{HomeField, HomeTab, ResolveState};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

use super::widgets;
use super::{HELP_CUSTOM_MIRROR, danger, mirror_label, success, text_dim, text_faint, text_muted};
use osu_downloader::MirrorKind;

const PANEL_TITLE: &str = " HOME ";

const SECTION_COLLECTION: &str = "collection";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_DOWNLOAD: &str = "download";
/// Sentinel for a field that belongs to no section (the download button);
/// never equals a rendered header label, so no title lights up.
const SECTION_NONE: &str = "";

const LABEL_OVERWRITE: &str = "overwrite existing";
const LABEL_VIDEO: &str = "video";

const LABEL_START_DOWNLOAD: &str = "start download";

/// Returns the terminal caret position when a text field is focused, for the
/// caller to apply. `None` when no caret should show (non-text focus).
///
/// System-wide banners are rendered by [`super::draw`] above the body area, so
/// this receives the already-reduced content area.
pub fn render(frame: &mut Frame, area: Rect, form: &HomeTab, editing: bool) -> Option<(u16, u16)> {
    if area.height < super::COMPACT_HEIGHT {
        return render_compact(frame, area, form, editing);
    }
    render_content(frame, area, form, editing)
}

/// Compact render: all focusable fields without section headers, spacers, or help lines.
///
/// Navigation is identical to normal mode — the full `HOME_FIELDS` cycle still applies.
/// Only decorative chrome is stripped to reclaim vertical space.
fn render_compact(
    frame: &mut Frame,
    area: Rect,
    form: &HomeTab,
    editing: bool,
) -> Option<(u16, u16)> {
    let focus = form.focus;
    let mut items = widgets::FormItems::new(focus);

    items.push_focusable(
        HomeField::Collection,
        widgets::input_item(&form.collection, focus == HomeField::Collection, editing),
    );
    if let Some((state, text)) = &form.collection_resolve {
        items.push(resolve_row(*state, text));
    }

    items.push_focusable(
        HomeField::CustomMirror,
        widgets::input_item(
            &form.custom_mirror,
            focus == HomeField::CustomMirror,
            editing,
        ),
    );

    push_mirror_rows(&mut items, form, focus);

    items.push_focusable(
        HomeField::Directory,
        widgets::input_item(&form.directory, focus == HomeField::Directory, editing),
    );
    items.push_focusable(
        HomeField::Threads,
        widgets::stepper_item(
            form.threads.label,
            form.resolved_threads(),
            form.default_threads,
            focus == HomeField::Threads,
        ),
    );
    push_toggle_rows(&mut items, form, focus);

    items.push_focusable(
        HomeField::Download,
        widgets::button_item(
            LABEL_START_DOWNLOAD,
            focus == HomeField::Download,
            can_download(form),
        ),
    );

    let cursor_col = editing
        .then(|| form.focused_input().map(widgets::input_cursor_col))
        .flatten();
    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(
        frame,
        area,
        PANEL_TITLE,
        &items,
        focused_index,
        cursor_col,
        true,
        true,
    )
}

/// Whether the form has the minimum inputs a download needs: a collection
/// reference and at least one enabled mirror. Drives the button's enabled state;
/// final validation still happens in `HomeTab::build_request` on activation.
fn can_download(form: &HomeTab) -> bool {
    !form.collection.value.trim().is_empty() && form.mirror_count() > 0
}

fn render_content(
    frame: &mut Frame,
    area: Rect,
    form: &HomeTab,
    editing: bool,
) -> Option<(u16, u16)> {
    let focus = form.focus;
    let mut items = widgets::FormItems::new(focus);

    let active_section = home_section(focus);
    items.push(widgets::section_header(
        SECTION_COLLECTION,
        active_section == SECTION_COLLECTION,
    ));
    items.push_focusable(
        HomeField::Collection,
        widgets::input_item(&form.collection, focus == HomeField::Collection, editing),
    );
    if let Some((state, text)) = &form.collection_resolve {
        items.push(resolve_row(*state, text));
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(
        SECTION_MIRRORS,
        active_section == SECTION_MIRRORS,
    ));
    items.push_focusable(
        HomeField::CustomMirror,
        widgets::input_item(
            &form.custom_mirror,
            focus == HomeField::CustomMirror,
            editing,
        ),
    );
    if focus == HomeField::CustomMirror {
        items.push(widgets::help_item(HELP_CUSTOM_MIRROR));
    }

    push_mirror_rows(&mut items, form, focus);
    items.push(widgets::spacer());

    items.push(widgets::section_header(
        SECTION_DOWNLOAD,
        active_section == SECTION_DOWNLOAD,
    ));
    items.push_focusable(
        HomeField::Directory,
        widgets::input_item(&form.directory, focus == HomeField::Directory, editing),
    );
    items.push_focusable(
        HomeField::Threads,
        widgets::stepper_item(
            form.threads.label,
            form.resolved_threads(),
            form.default_threads,
            focus == HomeField::Threads,
        ),
    );
    push_toggle_rows(&mut items, form, focus);
    items.push(widgets::spacer());

    items.push_focusable(
        HomeField::Download,
        widgets::button_item(
            LABEL_START_DOWNLOAD,
            focus == HomeField::Download,
            can_download(form),
        ),
    );

    let cursor_col = editing
        .then(|| form.focused_input().map(widgets::input_cursor_col))
        .flatten();
    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(
        frame,
        area,
        PANEL_TITLE,
        &items,
        focused_index,
        cursor_col,
        true,
        true,
    )
}

/// Pushes the two boolean override toggles (`overwrite existing`, `video`),
/// shared by `render_compact` and `render_content`.
///
/// The slide-toggle glyph already encodes each row's state, so neither row
/// repeats it as text and neither carries a default hint.
fn push_toggle_rows(items: &mut widgets::FormItems<HomeField>, form: &HomeTab, focus: HomeField) {
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
        HomeField::Video,
        widgets::row_item(LABEL_VIDEO, None, form.video, focus == HomeField::Video),
    );
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

/// The section a focused field belongs to, driving the active-section header cue.
///
/// The download button sits below all sections, so it maps to no header
/// (`SECTION_NONE`): focusing it leaves every section title un-underlined.
fn home_section(field: HomeField) -> &'static str {
    use HomeField::*;
    match field {
        Collection => SECTION_COLLECTION,
        CustomMirror | MirrorOsuDirect | MirrorNerinyan | MirrorSayobot | MirrorNekoha => {
            SECTION_MIRRORS
        }
        Threads | AutoOverwrite | Video | Directory => SECTION_DOWNLOAD,
        Download => SECTION_NONE,
    }
}

/// Mirror toggle row: the shared [`widgets::row_item`] base plus a trailing
/// latency readout (see [`latency_span`]).
fn mirror_row_item(
    label: &str,
    host: &str,
    on: bool,
    focused: bool,
    latency: Option<Option<ProbeResult>>,
) -> ListItem<'static> {
    widgets::row_item_with_suffix(label, Some(host), on, focused, latency_span(latency))
}

/// The trailing latency readout appended to a mirror row, or `None` before the
/// first probe.
///
/// `latency` mirrors `HomeTab::mirror_latency` semantics:
/// - `None`          → not yet probed (no suffix)
/// - `Some(None)`    → probe in flight (`…`)
/// - `Some(Some(_))` → result received
fn latency_span(latency: Option<Option<ProbeResult>>) -> Option<Span<'static>> {
    match latency? {
        None => Some(Span::styled("  …", Style::default().fg(text_dim()))),
        Some(ProbeResult::Ms(ms)) => {
            let mut s = String::with_capacity(10);
            s.push_str("  ");
            s.push_str(&ms.to_string());
            s.push_str("ms");
            Some(Span::styled(s, Style::default().fg(success())))
        }
        Some(ProbeResult::Timeout) => {
            Some(Span::styled("  timeout", Style::default().fg(danger())))
        }
        Some(ProbeResult::Error) => Some(Span::styled("  N/A", Style::default().fg(danger()))),
    }
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
