use osu_collect::{
    app::{App, CollectionPage, ConfigField},
    config::{Config, constants::CONFIG_TAB_INDEX},
    download::{DownloadStage, DownloadSummary},
    tui,
};
use ratatui::{Terminal, backend::TestBackend, style::Color};

fn render_app(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| tui::draw(frame, app))
        .expect("app should render");

    terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect::<String>()
}

#[test]
fn home_render_shows_cloudy_sections_and_footer() {
    let app = App::new(Config::default());

    let output = render_app(&app, 80, 24);

    assert!(output.contains("[ home ]"));
    assert!(output.contains("COLLECTION"));
    assert!(output.contains("MIRRORS"));
    assert!(output.contains("DOWNLOAD"));
    assert!(output.contains("space toggles mirrors"));
    assert!(output.contains("enter download"));
}

#[test]
fn config_render_scrolls_to_focused_logging_field() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::LoggingDirectory;

    let output = render_app(&app, 40, 10);

    assert!(output.contains("LOGGING"));
    assert!(output.contains("logs directory"));
}

#[test]
fn config_render_shows_download_help() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadThreads;

    let output = render_app(&app, 80, 20);

    assert!(output.contains("defaults used by home and updates downloads"));
    assert!(output.contains("reject truncated archives"));
}

#[test]
fn config_render_shows_official_login_status() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::MirrorCustomUrl;

    let output = render_app(&app, 80, 24);

    assert!(output.contains("osu! login") || output.contains("OSU! LOGIN"));
    assert!(output.contains("not logged in") || output.contains("logged in"));
    assert!(!output.contains("client id:"));
    assert!(!output.contains("client secret:"));
}

#[test]
fn download_render_shows_status_metrics_and_results() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 2);
    page.stage = DownloadStage::Completed;
    page.total_maps = 10;
    page.download_target = 10;
    page.stats.downloaded = 8;
    page.stats.skipped = 2;
    page.summary = Some(DownloadSummary {
        downloaded: 8,
        skipped: 2,
        failed: 0,
        unverified: 0,
    });
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 90, 24);

    assert!(output.contains("completed"));
    assert!(output.contains("2 threads"));
    assert!(output.contains("8 downloaded"));
    assert!(output.contains("DOWNLOADED"));
    assert!(output.contains("SKIPPED"));
}

#[test]
fn active_home_tab_uses_orange_title_color() {
    let app = App::new(Config::default());
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| tui::draw(frame, &app))
        .expect("app should render");

    let has_accent_cell = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|cell| cell.symbol() == "[" && cell.style().fg == Some(Color::Rgb(217, 119, 87)));

    assert!(has_accent_cell);
}

#[test]
fn footer_info_message_uses_info_color() {
    let mut app = App::new(Config::default());
    app.home.set_info("ready");
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| tui::draw(frame, &app))
        .expect("app should render");

    let has_info_cell = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|cell| cell.symbol() == "i" && cell.style().fg == Some(Color::Rgb(116, 199, 236)));

    assert!(has_info_cell);
}
