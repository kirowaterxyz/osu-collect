use crate::{
    app::{ConfigField, ConfigTab},
    config::{LogFormat, LogLevel},
};
use ratatui::{Frame, layout::Rect, widgets::List};

use super::{ConfigView, components};

pub fn render(frame: &mut Frame, area: Rect, view: ConfigView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &ConfigTab) {
    let items = vec![
        components::section_header("mirrors"),
        components::toggle_item(
            "nerinyan (api.nerinyan.moe)",
            form.nerinyan,
            form.focus == ConfigField::MirrorNerinyan,
        ),
        components::toggle_item(
            "catboy central (catboy.best)",
            form.catboy_central,
            form.focus == ConfigField::MirrorCatboyCentral,
        ),
        components::toggle_item(
            "catboy us (us.catboy.best)",
            form.catboy_us,
            form.focus == ConfigField::MirrorCatboyUs,
        ),
        components::toggle_item(
            "catboy asia (sg.catboy.best)",
            form.catboy_asia,
            form.focus == ConfigField::MirrorCatboyAsia,
        ),
        components::toggle_item(
            "osu.direct (osu.direct)",
            form.osu_direct,
            form.focus == ConfigField::MirrorOsuDirect,
        ),
        components::toggle_item(
            "sayobot (dl.sayobot.cn)",
            form.sayobot,
            form.focus == ConfigField::MirrorSayobot,
        ),
        components::toggle_item(
            "nekoha (mirror.nekoha.moe)",
            form.nekoha,
            form.focus == ConfigField::MirrorNekoha,
        ),
        components::input_item(
            &form.custom_mirror,
            form.focus == ConfigField::MirrorCustomUrl,
        ),
        components::section_header("download"),
        components::toggle_item(
            "skip existing files",
            form.skip_existing,
            form.focus == ConfigField::DownloadSkipExisting,
        ),
        components::input_item(&form.threads, form.focus == ConfigField::DownloadThreads),
        components::input_item(&form.retries, form.focus == ConfigField::DownloadRetries),
        components::toggle_item(
            "no video",
            form.no_video,
            form.focus == ConfigField::DownloadNoVideo,
        ),
        components::toggle_item(
            "verify .osz integrity",
            form.verify_zip_eocd,
            form.focus == ConfigField::DownloadVerifyZipEocd,
        ),
        components::section_header("logging"),
        components::toggle_item(
            "enable logging",
            form.logging_enabled,
            form.focus == ConfigField::LoggingEnabled,
        ),
        components::cycle_item(
            "level",
            &["error", "warn", "info", "debug", "trace"],
            log_level_label(form.logging_level),
            form.focus == ConfigField::LoggingLevel,
        ),
        components::cycle_item(
            "format",
            &["compact", "pretty"],
            log_format_label(form.logging_format),
            form.focus == ConfigField::LoggingFormat,
        ),
        components::input_item(
            &form.logging_dir,
            form.focus == ConfigField::LoggingDirectory,
        ),
    ];

    let focused_index = match form.focus {
        ConfigField::MirrorNerinyan => 1,
        ConfigField::MirrorCatboyCentral => 2,
        ConfigField::MirrorCatboyUs => 3,
        ConfigField::MirrorCatboyAsia => 4,
        ConfigField::MirrorOsuDirect => 5,
        ConfigField::MirrorSayobot => 6,
        ConfigField::MirrorNekoha => 7,
        ConfigField::MirrorCustomUrl => 8,
        ConfigField::DownloadSkipExisting => 10,
        ConfigField::DownloadThreads => 11,
        ConfigField::DownloadRetries => 12,
        ConfigField::DownloadNoVideo => 13,
        ConfigField::DownloadVerifyZipEocd => 14,
        ConfigField::LoggingEnabled => 16,
        ConfigField::LoggingLevel => 17,
        ConfigField::LoggingFormat => 18,
        ConfigField::LoggingDirectory => 19,
    };

    let inner_block = components::panel_block("config");
    let inner = inner_block.inner(area);
    frame.render_widget(inner_block, area);

    let visible_height = inner.height as usize;
    let (start, end) = components::scroll_window(&items, focused_index, visible_height);
    let visible_items = items[start..end].to_vec();

    let list = List::new(visible_items).highlight_symbol("");
    frame.render_widget(list, inner);
}

fn log_level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Error => "error",
        LogLevel::Warn => "warn",
        LogLevel::Info => "info",
        LogLevel::Debug => "debug",
        LogLevel::Trace => "trace",
    }
}

fn log_format_label(format: LogFormat) -> &'static str {
    match format {
        LogFormat::Compact => "compact",
        LogFormat::Pretty => "pretty",
    }
}
