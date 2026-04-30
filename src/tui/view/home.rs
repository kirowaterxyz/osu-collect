use crate::app::{HomeField, HomeTab};
use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{List, ListItem},
};

use super::{HomeView, components};

pub fn render(frame: &mut Frame, area: Rect, view: HomeView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &HomeTab) {
    let items = vec![
        components::section_header("collection"),
        components::input_item(&form.collection, form.focus == HomeField::Collection),
        components::input_item(&form.directory, form.focus == HomeField::Directory),
        components::section_header("mirrors"),
        components::input_item(&form.custom_mirror, form.focus == HomeField::CustomMirror),
        mirror_toggle(
            "use nerinyan",
            "api.nerinyan.moe",
            form.nerinyan,
            form.focus == HomeField::MirrorNerinyan,
        ),
        mirror_toggle(
            "use osu.direct",
            "osu.direct",
            form.osu_direct,
            form.focus == HomeField::MirrorOsuDirect,
        ),
        mirror_toggle(
            "use sayobot",
            "dl.sayobot.cn",
            form.sayobot,
            form.focus == HomeField::MirrorSayobot,
        ),
        mirror_toggle(
            "use nekoha",
            "mirror.nekoha.moe",
            form.nekoha,
            form.focus == HomeField::MirrorNekoha,
        ),
        mirror_toggle(
            "use catboy central",
            "catboy.best",
            form.catboy_central,
            form.focus == HomeField::MirrorCatboyCentral,
        ),
        mirror_toggle(
            "use catboy us",
            "us.catboy.best",
            form.catboy_us,
            form.focus == HomeField::MirrorCatboyUs,
        ),
        mirror_toggle(
            "use catboy asia",
            "sg.catboy.best",
            form.catboy_asia,
            form.focus == HomeField::MirrorCatboyAsia,
        ),
        components::section_header("download"),
        components::input_item(&form.threads, form.focus == HomeField::Threads),
        components::toggle_item(
            "skip existing files",
            form.skip_existing,
            form.focus == HomeField::SkipExisting,
        ),
        components::toggle_item(
            "auto-overwrite",
            form.auto_overwrite,
            form.focus == HomeField::AutoOverwrite,
        ),
        components::toggle_item(
            "download without video",
            form.no_video,
            form.focus == HomeField::NoVideo,
        ),
    ];

    let focused_index = match form.focus {
        HomeField::Collection => 1,
        HomeField::Directory => 2,
        HomeField::CustomMirror => 4,
        HomeField::MirrorNerinyan => 5,
        HomeField::MirrorOsuDirect => 6,
        HomeField::MirrorSayobot => 7,
        HomeField::MirrorNekoha => 8,
        HomeField::MirrorCatboyCentral => 9,
        HomeField::MirrorCatboyUs => 10,
        HomeField::MirrorCatboyAsia => 11,
        HomeField::Threads => 13,
        HomeField::SkipExisting => 14,
        HomeField::AutoOverwrite => 15,
        HomeField::NoVideo => 16,
    };

    let inner_block = components::panel_block("home");
    let inner = inner_block.inner(area);
    frame.render_widget(inner_block, area);

    let visible_height = inner.height as usize;
    let (start, end) = components::scroll_window(&items, focused_index, visible_height);
    let visible_items = items[start..end].to_vec();

    let list = List::new(visible_items).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn mirror_toggle(label: &str, url: &str, value: bool, focused: bool) -> ListItem<'static> {
    let (marker, marker_style) = components::check_marker(value);
    let spans = vec![
        components::focus_span(focused),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {label}  "), Style::default().fg(components::TEXT)),
        Span::styled(url.to_string(), Style::default().fg(components::TEXT_FAINT)),
    ];

    ListItem::new(Line::from(spans))
}
