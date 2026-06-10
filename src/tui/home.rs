use crate::app::runtime::ProbeResult;
use crate::app::{Banner, HomeField, HomeTab, ResolveState};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

use super::banner::{banner_height, render_banners};
use super::widgets::{self, Metric};
use super::{HELP_CUSTOM_MIRROR, danger, mirror_label, success, text_dim, text_faint, text_muted};
use osu_downloader::MirrorKind;

const PANEL_TITLE: &str = " HOME ";

const SECTION_COLLECTION: &str = "collection";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_DOWNLOAD: &str = "download";

const LABEL_OVERWRITE: &str = "overwrite existing";
const LABEL_NO_VIDEO: &str = "no video";

const METRIC_THREADS: &str = "threads";
const METRIC_MIRRORS: &str = "mirrors";

const LABEL_START_DOWNLOAD: &str = "start download";

/// Returns the terminal caret position when a text field is focused, for the
/// caller to apply. `None` when no caret should show (non-text focus).
pub fn render(
    frame: &mut Frame,
    area: Rect,
    form: &HomeTab,
    banners: &[Banner],
) -> Option<(u16, u16)> {
    if area.height < super::COMPACT_HEIGHT {
        return render_compact(frame, area, form);
    }

    let (banner_area, content_area) = split_banner_area(area, banners);
    render_banners(frame, banner_area, banners);
    render_content(frame, content_area, form)
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
    push_toggle_rows(&mut items, form, focus);

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
    push_toggle_rows(&mut items, form, focus);
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

/// Pushes the two boolean override toggles (`overwrite existing`, `no video`),
/// shared by `render_compact` and `render_content`.
///
/// The check glyph already encodes each toggle's state, so neither row repeats
/// it as text. `no_video` overrides the saved `download.no_video` default, so it
/// carries a dim `(default: on/off)` hint sourced from `HomeTab::default_no_video`.
/// `auto_overwrite` has no config default, so it gets no detail.
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
        HomeField::NoVideo,
        widgets::row_item(
            LABEL_NO_VIDEO,
            Some(&default_hint(form.default_no_video)),
            form.no_video,
            focus == HomeField::NoVideo,
        ),
    );
}

/// A `(default: on)` / `(default: off)` hint string for a home override toggle.
fn default_hint(value: bool) -> String {
    if value {
        "(default: on)".to_string()
    } else {
        "(default: off)".to_string()
    }
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
            s.push_str("  ✓ ");
            s.push_str(&ms.to_string());
            s.push_str("ms");
            Some(Span::styled(s, Style::default().fg(success())))
        }
        Some(ProbeResult::Timeout) => {
            Some(Span::styled("  ✗timeout", Style::default().fg(danger())))
        }
        Some(ProbeResult::Error) => Some(Span::styled("  ✗N/A", Style::default().fg(danger()))),
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
