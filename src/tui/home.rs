use crate::app::{HomeField, HomeTab};
use ratatui::{Frame, layout::Rect, widgets::ListItem};

use super::widgets::{self, Metric};
use super::{HELP_CUSTOM_MIRROR, MIRRORS};

const PANEL_TITLE: &str = "home";

const SECTION_COLLECTION: &str = "collection";
const SECTION_MIRRORS: &str = "mirrors";
const SECTION_DOWNLOAD: &str = "download";

const LABEL_OVERWRITE: &str = "overwrite existing";
const LABEL_NO_VIDEO: &str = "no video";

const METRIC_THREADS: &str = "threads";
const METRIC_MIRRORS: &str = "mirrors";

pub fn render(frame: &mut Frame, area: Rect, form: &HomeTab) {
    let mut items: Vec<ListItem<'static>> = Vec::new();
    let mut focused_index = 0usize;
    let focus = form.focus;

    let push = |items: &mut Vec<ListItem<'static>>,
                idx: &mut usize,
                field: HomeField,
                item: ListItem<'static>| {
        if focus == field {
            *idx = items.len();
        }
        items.push(item);
    };

    items.push(widgets::section_header(SECTION_COLLECTION));
    push(
        &mut items,
        &mut focused_index,
        HomeField::Collection,
        widgets::input_item(&form.collection, focus == HomeField::Collection),
    );
    push(
        &mut items,
        &mut focused_index,
        HomeField::Directory,
        widgets::input_item(&form.directory, focus == HomeField::Directory),
    );
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_MIRRORS));
    push(
        &mut items,
        &mut focused_index,
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
        push(
            &mut items,
            &mut focused_index,
            field,
            widgets::row_item(label, Some(url), on, focus == field),
        );
    }
    items.push(widgets::spacer());

    items.push(widgets::section_header(SECTION_DOWNLOAD));
    push(
        &mut items,
        &mut focused_index,
        HomeField::Threads,
        widgets::input_item(&form.threads, focus == HomeField::Threads),
    );
    push(
        &mut items,
        &mut focused_index,
        HomeField::AutoOverwrite,
        widgets::row_item(
            LABEL_OVERWRITE,
            None,
            form.auto_overwrite,
            focus == HomeField::AutoOverwrite,
        ),
    );
    push(
        &mut items,
        &mut focused_index,
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
        Metric::accent(METRIC_MIRRORS, form.build_mirrors().len().to_string()),
    ]));

    widgets::render_scrollable_panel(frame, area, PANEL_TITLE, &items, focused_index);
}
