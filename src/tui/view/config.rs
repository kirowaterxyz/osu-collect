use crate::{
    app::{ConfigField, ConfigTab},
    config::{LogFormat, LogLevel, constants::DEFAULT_THREADS},
};
use ratatui::{Frame, layout::Rect, widgets::List};

use super::{ConfigView, components};

pub fn render(frame: &mut Frame, area: Rect, view: ConfigView) {
    render_form(frame, area, view.form);
}

fn render_form(frame: &mut Frame, area: Rect, form: &ConfigTab) {
    let items = vec![
        components::section_header("mirrors"),
        components::help_item("space toggles mirrors; custom URL must contain {id}"),
        components::row_item(
            "nerinyan",
            Some("api.nerinyan.moe"),
            form.nerinyan,
            form.focus == ConfigField::MirrorNerinyan,
        ),
        components::row_item(
            "catboy central",
            Some("catboy.best"),
            form.catboy_central,
            form.focus == ConfigField::MirrorCatboyCentral,
        ),
        components::row_item(
            "catboy us",
            Some("us.catboy.best"),
            form.catboy_us,
            form.focus == ConfigField::MirrorCatboyUs,
        ),
        components::row_item(
            "catboy asia",
            Some("sg.catboy.best"),
            form.catboy_asia,
            form.focus == ConfigField::MirrorCatboyAsia,
        ),
        components::row_item(
            "osu!direct",
            Some("osu.direct"),
            form.osu_direct,
            form.focus == ConfigField::MirrorOsuDirect,
        ),
        components::row_item(
            "sayobot",
            Some("dl.sayobot.cn"),
            form.sayobot,
            form.focus == ConfigField::MirrorSayobot,
        ),
        components::row_item(
            "nekoha",
            Some("mirror.nekoha.moe"),
            form.nekoha,
            form.focus == ConfigField::MirrorNekoha,
        ),
        components::row_item(
            "official",
            Some("requires login"),
            form.official,
            form.focus == ConfigField::MirrorOfficial,
        ),
        components::input_item(
            &form.custom_mirror,
            form.focus == ConfigField::MirrorCustomUrl,
        ),
        components::input_item(
            &form.official_client_id,
            form.focus == ConfigField::OfficialClientId,
        ),
        components::input_item(
            &form.official_client_secret,
            form.focus == ConfigField::OfficialClientSecret,
        ),
        components::spacer(),
        components::section_header("download"),
        components::help_item("defaults used by home and updates downloads"),
        components::row_item(
            "skip existing files",
            Some("keep files already on disk"),
            form.skip_existing,
            form.focus == ConfigField::DownloadSkipExisting,
        ),
        components::input_item(&form.threads, form.focus == ConfigField::DownloadThreads),
        components::input_item(&form.retries, form.focus == ConfigField::DownloadRetries),
        components::row_item(
            "skip videos",
            Some("smaller downloads"),
            form.no_video,
            form.focus == ConfigField::DownloadNoVideo,
        ),
        components::row_item(
            "verify .osz integrity",
            Some("reject truncated archives"),
            form.verify_zip_eocd,
            form.focus == ConfigField::DownloadVerifyZipEocd,
        ),
        components::summary_item(&[
            components::Metric::accent("threads", configured_or_default(&form.threads.value)),
            components::Metric::muted("retries", configured_or_default(&form.retries.value)),
            components::Metric::muted("default", DEFAULT_THREADS.to_string()),
        ]),
        components::spacer(),
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
        ConfigField::MirrorNerinyan => 2,
        ConfigField::MirrorCatboyCentral => 3,
        ConfigField::MirrorCatboyUs => 4,
        ConfigField::MirrorCatboyAsia => 5,
        ConfigField::MirrorOsuDirect => 6,
        ConfigField::MirrorSayobot => 7,
        ConfigField::MirrorNekoha => 8,
        ConfigField::MirrorOfficial => 9,
        ConfigField::MirrorCustomUrl => 10,
        ConfigField::OfficialClientId => 11,
        ConfigField::OfficialClientSecret => 12,
        ConfigField::DownloadSkipExisting => 16,
        ConfigField::DownloadThreads => 17,
        ConfigField::DownloadRetries => 18,
        ConfigField::DownloadNoVideo => 19,
        ConfigField::DownloadVerifyZipEocd => 20,
        ConfigField::LoggingEnabled => 24,
        ConfigField::LoggingLevel => 25,
        ConfigField::LoggingFormat => 26,
        ConfigField::LoggingDirectory => 27,
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

fn configured_or_default(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        "default".to_string()
    } else {
        trimmed.to_string()
    }
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
