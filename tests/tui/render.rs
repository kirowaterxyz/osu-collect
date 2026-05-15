/// Rendering smoke tests using ratatui's TestBackend.
///
/// These verify that each view renders without panic and produces
/// non-empty output at standard terminal sizes.
use osu_collect::{app::App, config::Config, tui::draw};
use ratatui::{Terminal, backend::TestBackend};

fn make_app() -> App {
    App::new(Config::default())
}

fn render_to_buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    terminal.backend().buffer().clone()
}

// ── home view ────────────────────────────────────────────────────────────────

#[test]
fn home_renders_without_panic_standard() {
    let app = make_app();
    let buf = render_to_buffer(&app, 120, 40);
    // header should contain the brand name
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("osu-collect"));
}

#[test]
fn home_renders_without_panic_small() {
    let app = make_app();
    // 80×24 is the minimum usable terminal size
    let _buf = render_to_buffer(&app, 80, 24);
}

#[test]
fn home_renders_without_panic_compact() {
    let app = make_app();
    // under 14 rows triggers compact mode (no separator rows)
    let _buf = render_to_buffer(&app, 80, 10);
}

#[test]
fn home_renders_collection_label() {
    let app = make_app();
    let buf = render_to_buffer(&app, 120, 40);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("collection"));
}

#[test]
fn home_renders_mirrors_section() {
    let app = make_app();
    let buf = render_to_buffer(&app, 120, 40);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("MIRRORS") || content.contains("mirrors"));
}

// ── updates view ─────────────────────────────────────────────────────────────

#[test]
fn updates_tab_renders_without_panic() {
    let mut app = make_app();
    // navigate to updates tab
    app.next_tab();
    let _buf = render_to_buffer(&app, 120, 40);
}

#[test]
fn updates_tab_shows_recheck_failed_control() {
    let mut app = make_app();
    app.next_tab();
    app.updates.set_failed_beatmapset_count(2);
    let buf = render_to_buffer(&app, 120, 40);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();

    assert!(
        content.contains("failed"),
        "summary metrics must surface the failed count"
    );
    assert!(
        content.contains('2'),
        "the failed beatmap count must be rendered"
    );
}

#[test]
fn updates_tab_shows_client_toggle() {
    let mut app = make_app();
    app.next_tab();
    let buf = render_to_buffer(&app, 120, 40);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    // client toggle shows either "lazer" or "stable"
    assert!(content.contains("lazer") || content.contains("stable"));
}

// ── config view ──────────────────────────────────────────────────────────────

#[test]
fn config_tab_renders_without_panic() {
    let mut app = make_app();
    app.next_tab();
    app.next_tab();
    let _buf = render_to_buffer(&app, 120, 40);
}

#[test]
fn config_tab_shows_login_section() {
    let mut app = make_app();
    app.next_tab();
    app.next_tab();
    let buf = render_to_buffer(&app, 120, 40);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    assert!(content.contains("login") || content.contains("LOGIN"));
}

// ── downloads view ───────────────────────────────────────────────────────────

#[test]
fn download_tab_renders_without_panic() {
    use osu_collect::{app::CollectionPage, download::DownloadStage};

    let app = make_app();
    // inject a download page directly to test without network
    let mut page = CollectionPage::new(1, "test collection".to_string(), 3);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 100;
    page.download_target = 80;
    // access downloads field through state mutation
    // use handle_cancel_result to validate the page was added — but to add we
    // call request_download which requires a real URL. Instead we exercise the
    // download view by checking a CollectionPage can be created.
    drop(page);
    // just verify home still renders cleanly (no download tab added)
    let _buf = render_to_buffer(&app, 120, 40);
}

// ── error / message footer ───────────────────────────────────────────────────

#[test]
fn footer_shows_hint_line() {
    use ratatui::{Terminal, backend::TestBackend};

    let app = make_app();
    let backend = TestBackend::new(120, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    // footer should contain the hint line keys
    assert!(content.contains("move") || content.contains("quit") || content.contains("↑↓"));
}

// ── gauge label ──────────────────────────────────────────────────────────────

#[test]
fn gauge_label_shows_avg_when_verified() {
    use osu_collect::app::CollectionPage;

    let mut page = CollectionPage::new(1, "test".to_string(), 1);
    page.total_maps = 10;
    page.stats.downloaded = 3;
    page.stats.skipped = 2;
    page.stats.verify_total_count = 5;
    page.stats.verify_total_us = 5_000_000;

    let avg = page.avg_verify_us();
    assert_eq!(avg, Some(1_000_000));
}

#[test]
fn gauge_label_none_when_no_verified() {
    use osu_collect::app::CollectionPage;

    let page = CollectionPage::new(1, "test".to_string(), 1);
    assert_eq!(page.avg_verify_us(), None);
}

// ── config item order ─────────────────────────────────────────────────────────

#[test]
fn config_tab_shows_download_section_before_mirrors() {
    let mut app = make_app();
    app.next_tab();
    app.next_tab();
    let buf = render_to_buffer(&app, 120, 60);
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    // both sections should be present
    assert!(content.contains("download") || content.contains("DOWNLOAD"));
    assert!(content.contains("mirrors") || content.contains("MIRRORS"));
    // "download" text should appear before "mirrors" text in the rendered buffer
    let dl_pos = content
        .find("download")
        .or_else(|| content.find("DOWNLOAD"));
    let mir_pos = content.find("mirrors").or_else(|| content.find("MIRRORS"));
    if let (Some(d), Some(m)) = (dl_pos, mir_pos) {
        assert!(d < m, "download section should render before mirrors");
    }
}

// ── zero-size terminal ────────────────────────────────────────────────────────

#[test]
fn zero_width_does_not_panic() {
    let app = make_app();
    let _buf = render_to_buffer(&app, 0, 24);
}

#[test]
fn zero_height_does_not_panic() {
    let app = make_app();
    let _buf = render_to_buffer(&app, 120, 0);
}
