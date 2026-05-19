use crate::app::{HomeField, HomeTab};
use ratatui::{Frame, layout::Rect};

use super::widgets::{self, Metric};
use super::{HELP_CUSTOM_MIRROR, MIRRORS};

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
    for ((label, url), (field, on)) in MIRRORS.iter().zip(mirror_states) {
        items.push_focusable(
            field,
            widgets::row_item(label, Some(url), on, focus == field),
        );
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_DOWNLOAD));
    items.push_focusable(
        HomeField::Threads,
        widgets::input_item(&form.threads, focus == HomeField::Threads),
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
