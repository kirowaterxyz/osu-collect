use crate::app::runtime::ProbeResult;
use crate::app::{HomeField, HomeTab, ResolveState};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::ListItem,
};

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

pub fn render(frame: &mut Frame, area: Rect, form: &HomeTab) {
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
    items.push(widgets::help_item(HELP_CUSTOM_MIRROR));

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

    items.push(widgets::summary_item(&[
        Metric::accent(METRIC_THREADS, form.resolved_threads().to_string()),
        Metric::accent(METRIC_MIRRORS, form.build_mirror_list().len().to_string()),
    ]));

    let (items, focused_index) = items.into_parts();
    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index);
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
            s.push_str("  \u{2713} ");
            s.push_str(&ms.to_string());
            s.push_str("ms");
            spans.push(Span::styled(s, Style::default().fg(success())));
        }
        Some(Some(ProbeResult::Timeout)) => {
            spans.push(Span::styled(
                "  \u{2717} timeout",
                Style::default().fg(danger()),
            ));
        }
        Some(Some(ProbeResult::Error)) => {
            spans.push(Span::styled(
                "  \u{2717} N/A",
                Style::default().fg(danger()),
            ));
        }
    }

    ListItem::new(Line::from(spans))
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
