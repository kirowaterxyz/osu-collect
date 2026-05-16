use crate::app::{HomeField, HomeTab};
use ratatui::{Frame, layout::Rect};

use super::{HomeView, components};

pub fn render(frame: &mut Frame, area: Rect, view: HomeView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &HomeTab) {
    let items = vec![
        components::section_header("collection"),
        components::input_item(&form.collection, form.focus == HomeField::Collection),
        components::input_item(&form.directory, form.focus == HomeField::Directory),
        components::spacer(),
        components::section_header("mirrors"),
        components::input_item(&form.custom_mirror, form.focus == HomeField::CustomMirror),
        components::help_item("must contain {id}"),
        components::mirror_item(
            "osu!direct",
            "osu.direct",
            form.osu_direct,
            form.focus == HomeField::MirrorOsuDirect,
        ),
        components::mirror_item(
            "nerinyan",
            "api.nerinyan.moe",
            form.nerinyan,
            form.focus == HomeField::MirrorNerinyan,
        ),
        components::mirror_item(
            "sayobot",
            "dl.sayobot.cn",
            form.sayobot,
            form.focus == HomeField::MirrorSayobot,
        ),
        components::mirror_item(
            "nekoha",
            "mirror.nekoha.moe",
            form.nekoha,
            form.focus == HomeField::MirrorNekoha,
        ),
        components::spacer(),
        components::section_header("download"),
        components::input_item(&form.threads, form.focus == HomeField::Threads),
        components::row_item(
            "skip existing",
            None,
            form.skip_existing,
            form.focus == HomeField::SkipExisting,
        ),
        components::row_item(
            "overwrite existing",
            None,
            form.auto_overwrite,
            form.focus == HomeField::AutoOverwrite,
        ),
        components::row_item(
            "no video",
            None,
            form.no_video,
            form.focus == HomeField::NoVideo,
        ),
        components::spacer(),
        components::summary_item(&[
            components::Metric::accent("threads", form.resolved_threads().to_string()),
            components::Metric::accent("mirrors", form.build_mirrors().len().to_string()),
        ]),
    ];

    let focused_index = match form.focus {
        HomeField::Collection => 1,
        HomeField::Directory => 2,
        HomeField::CustomMirror => 5,
        HomeField::MirrorOsuDirect => 7,
        HomeField::MirrorNerinyan => 8,
        HomeField::MirrorSayobot => 9,
        HomeField::MirrorNekoha => 10,
        HomeField::Threads => 13,
        HomeField::SkipExisting => 14,
        HomeField::AutoOverwrite => 15,
        HomeField::NoVideo => 16,
    };

    components::render_scrollable_panel(frame, area, "home", &items, focused_index);
}
