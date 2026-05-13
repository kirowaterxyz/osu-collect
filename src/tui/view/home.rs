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
        components::spacer(),
        components::section_header("mirrors"),
        components::help_item("space toggles mirrors; custom URL must contain {id}"),
        components::input_item(&form.custom_mirror, form.focus == HomeField::CustomMirror),
        mirror_toggle(
            "nerinyan",
            "api.nerinyan.moe",
            form.nerinyan,
            form.focus == HomeField::MirrorNerinyan,
        ),
        mirror_toggle(
            "osu!direct",
            "osu.direct",
            form.osu_direct,
            form.focus == HomeField::MirrorOsuDirect,
        ),
        mirror_toggle(
            "sayobot",
            "dl.sayobot.cn",
            form.sayobot,
            form.focus == HomeField::MirrorSayobot,
        ),
        mirror_toggle(
            "nekoha",
            "mirror.nekoha.moe",
            form.nekoha,
            form.focus == HomeField::MirrorNekoha,
        ),
        mirror_toggle(
            "catboy central",
            "catboy.best",
            form.catboy_central,
            form.focus == HomeField::MirrorCatboyCentral,
        ),
        mirror_toggle(
            "catboy us",
            "us.catboy.best",
            form.catboy_us,
            form.focus == HomeField::MirrorCatboyUs,
        ),
        mirror_toggle(
            "catboy asia",
            "sg.catboy.best",
            form.catboy_asia,
            form.focus == HomeField::MirrorCatboyAsia,
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
            components::Metric::muted("retries", form.resolved_retries().to_string()),
        ]),
    ];

    let focused_index = match form.focus {
        HomeField::Collection => 1,
        HomeField::Directory => 2,
        HomeField::CustomMirror => 6,
        HomeField::MirrorNerinyan => 7,
        HomeField::MirrorOsuDirect => 8,
        HomeField::MirrorSayobot => 9,
        HomeField::MirrorNekoha => 10,
        HomeField::MirrorCatboyCentral => 11,
        HomeField::MirrorCatboyUs => 12,
        HomeField::MirrorCatboyAsia => 13,
        HomeField::Threads => 17,
        HomeField::SkipExisting => 18,
        HomeField::AutoOverwrite => 19,
        HomeField::NoVideo => 20,
    };

    let inner_block = components::panel_block("home");
    let inner = inner_block.inner(area);
    frame.render_widget(inner_block, area);

    let visible_height = inner.height as usize;
    let (start, end) = components::scroll_window(&items, focused_index, visible_height);
    if items.len() > visible_height {
        components::render_scroll_indicator(frame, inner, start, items.len());
    }
    let list = List::new(items[start..end].to_vec()).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn mirror_toggle(label: &str, url: &str, value: bool, focused: bool) -> ListItem<'static> {
    let (marker, marker_style) = components::check_marker(value);
    let label_style = if focused {
        Style::default()
            .fg(components::TEXT)
            .add_modifier(ratatui::style::Modifier::BOLD)
    } else {
        Style::default().fg(components::TEXT_MUTED)
    };
    let spans = vec![
        components::focus_span(focused),
        Span::styled(marker, marker_style),
        Span::styled(format!(" {label}"), label_style),
        Span::styled(
            format!("  {url}"),
            Style::default().fg(components::TEXT_FAINT),
        ),
    ];

    ListItem::new(Line::from(spans))
}
