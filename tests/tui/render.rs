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

fn render_content(app: &App, width: u16, height: u16) -> String {
    render_to_buffer(app, width, height)
        .content()
        .iter()
        .map(|c| c.symbol())
        .collect()
}

// ── home view ────────────────────────────────────────────────────────────────

#[test]
fn home_renders_without_panic_standard() {
    let app = make_app();
    let content = render_content(&app, 120, 40);
    assert!(content.contains("osu-collect"));
}

#[test]
fn home_renders_collection_label() {
    let app = make_app();
    let content = render_content(&app, 120, 40);
    assert!(content.contains("collection"));
}

#[test]
fn home_renders_mirrors_section() {
    let app = make_app();
    let content = render_content(&app, 120, 40);
    assert!(content.contains("MIRRORS") || content.contains("mirrors"));
}

// ── updates view ─────────────────────────────────────────────────────────────

#[test]
fn updates_tab_shows_recheck_failed_control() {
    let mut app = make_app();
    app.next_tab();
    app.updates.set_failed_beatmapset_count(2);
    let content = render_content(&app, 120, 40);

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
    let content = render_content(&app, 120, 40);
    // client toggle shows either "lazer" or "stable"
    assert!(content.contains("lazer") || content.contains("stable"));
}

// ── config view ──────────────────────────────────────────────────────────────

#[test]
fn config_tab_shows_login_section() {
    let mut app = make_app();
    app.next_tab();
    app.next_tab();
    let content = render_content(&app, 120, 40);
    assert!(content.contains("login") || content.contains("LOGIN"));
}

// ── error / message footer ───────────────────────────────────────────────────

#[test]
fn footer_shows_hint_line() {
    let app = make_app();
    let content = render_content(&app, 120, 24);
    // footer should contain the hint line keys
    assert!(content.contains("move") || content.contains("quit") || content.contains("↑↓"));
}

#[test]
fn home_footer_hides_space_on_text_input_focus() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::Collection;
    let content = render_content(&app, 120, 24);
    assert!(
        !content.contains("space toggle"),
        "space toggle hint must be hidden while a text field is focused"
    );
}

#[test]
fn home_footer_shows_space_on_toggle_focus() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::AutoOverwrite;
    let content = render_content(&app, 120, 24);
    assert!(content.contains("space toggle"));
}

#[test]
fn updates_footer_hides_recheck_without_failed_maps() {
    let mut app = make_app();
    app.next_tab();
    let content = render_content(&app, 120, 24);
    assert!(!content.contains("recheck"));
}

#[test]
fn updates_footer_in_list_shows_scroll_and_back() {
    let mut app = make_app();
    app.next_tab();
    app.updates.selection.in_collection_list = true;
    let content = render_content(&app, 120, 24);
    assert!(content.contains("scroll"));
    assert!(content.contains("esc"));
}

#[test]
fn config_footer_omits_space_on_text_input() {
    use osu_collect::app::ConfigField;
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::empty(),
    ));
    app.handle_key(crossterm::event::KeyEvent::new(
        crossterm::event::KeyCode::Right,
        crossterm::event::KeyModifiers::empty(),
    ));
    assert_eq!(app.active_tab(), CONFIG_TAB_INDEX);
    app.config.focus = ConfigField::MirrorCustomUrl;

    let content = render_content(&app, 120, 24);
    assert!(!content.contains("space change"));
    assert!(!content.contains("enter confirm"));
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

#[test]
fn gauge_label_none_when_avg_rounds_to_zero() {
    use osu_collect::app::CollectionPage;

    let mut page = CollectionPage::new(1, "test".to_string(), 1);
    page.stats.verify_total_count = 5;
    page.stats.verify_total_us = 0;
    assert_eq!(page.avg_verify_us(), None);
}

// ── config item order ─────────────────────────────────────────────────────────

#[test]
fn config_tab_shows_download_section_before_mirrors() {
    let mut app = make_app();
    app.next_tab();
    app.next_tab();
    let content = render_content(&app, 120, 60);
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
