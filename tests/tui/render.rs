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

/// Terminal caret position after a draw. `(0, 0)` means no caret was set this
/// frame (a focused text field always parks the caret inside the panel, y > 0).
fn cursor_pos(app: &App, width: u16, height: u16) -> (u16, u16) {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, app)).unwrap();
    let pos = terminal.get_cursor_position().unwrap();
    (pos.x, pos.y)
}

#[test]
fn caret_advances_as_collection_field_is_typed() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use osu_collect::app::HomeField;
    let mut app = make_app();
    app.home.focus = HomeField::Collection;

    app.home.collection.set_value("");
    let empty = cursor_pos(&app, 120, 24);

    // Type through the key handler so the caret advances with each char.
    for ch in "abcde".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()));
    }
    let typed = cursor_pos(&app, 120, 24);

    assert_eq!(typed.1, empty.1, "caret stays on the same row");
    assert_eq!(
        typed.0,
        empty.0 + 5,
        "caret advances one column per typed char"
    );
}

#[test]
fn caret_follows_left_arrow_then_home_and_end() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use osu_collect::app::HomeField;
    let mut app = make_app();
    app.home.focus = HomeField::Collection;
    app.home.collection.set_value("");
    let origin = cursor_pos(&app, 120, 24);

    for ch in "abcde".chars() {
        app.handle_key(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::empty()));
    }

    // Two lefts park the caret three chars in.
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::empty()));
    app.handle_key(KeyEvent::new(KeyCode::Left, KeyModifiers::empty()));
    assert_eq!(
        cursor_pos(&app, 120, 24).0,
        origin.0 + 3,
        "two left arrows move the caret back two columns"
    );

    app.handle_key(KeyEvent::new(KeyCode::Home, KeyModifiers::empty()));
    assert_eq!(
        cursor_pos(&app, 120, 24).0,
        origin.0,
        "Home parks the caret at the value start"
    );

    app.handle_key(KeyEvent::new(KeyCode::End, KeyModifiers::empty()));
    assert_eq!(
        cursor_pos(&app, 120, 24).0,
        origin.0 + 5,
        "End parks the caret at the value end"
    );
}

#[test]
fn no_caret_on_toggle_field() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    app.home.focus = HomeField::NoVideo;
    assert_eq!(
        cursor_pos(&app, 120, 24),
        (0, 0),
        "no caret is shown when a non-text field is focused"
    );
}

#[test]
fn no_caret_while_help_overlay_open() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    app.home.focus = HomeField::Collection;
    app.help_open = true;
    assert_eq!(
        cursor_pos(&app, 120, 24),
        (0, 0),
        "the help overlay suppresses the text caret"
    );
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
    // use a tall terminal so the summary_metrics row (last in the list) stays visible
    let content = render_content(&app, 120, 60);

    assert!(
        content.contains("FAILED") || content.contains("failed"),
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
fn config_tab_shows_auth_chip() {
    let mut app = make_app();
    app.next_tab();
    app.next_tab();
    let content = render_content(&app, 120, 40);
    assert!(
        content.contains("signed out")
            || content.contains("signed in")
            || content.contains("log in")
            || content.contains("login unavailable"),
        "auth chip must render a visible auth state: {content}"
    );
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
fn home_footer_hides_toggle_hint_on_text_input_focus() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::Collection;
    let content = render_content(&app, 120, 24);
    assert!(
        !content.contains("enter toggle"),
        "toggle hint must be hidden while a text field is focused"
    );
}

#[test]
fn home_footer_shows_enter_toggle_on_toggle_focus() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::AutoOverwrite;
    let content = render_content(&app, 120, 24);
    assert!(content.contains("enter toggle"));
}

#[test]
fn home_footer_shows_enter_download_on_button_focus() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::Download;
    let content = render_content(&app, 120, 24);
    assert!(content.contains("enter download"));
}

#[test]
fn updates_footer_hides_recheck_without_failed_maps() {
    let mut app = make_app();
    app.next_tab();
    let content = render_content(&app, 120, 24);
    assert!(!content.contains("recheck"));
}

#[test]
fn updates_footer_in_list_shows_scroll_and_select_hints() {
    let mut app = make_app();
    app.next_tab();
    app.updates.selection.in_collection_list = true;
    let content = render_content(&app, 120, 24);
    assert!(
        content.contains("scroll"),
        "in-list footer must show scroll hint"
    );
    assert!(
        content.contains("enter toggle"),
        "in-list footer must show enter toggle hint"
    );
    assert!(
        content.contains("all") && content.contains("none"),
        "in-list footer must show select-all / select-none hint"
    );
    assert!(
        content.contains('?'),
        "in-list footer must show ? help hint"
    );
}

#[test]
fn config_footer_omits_space_on_text_input() {
    use osu_collect::app::{ConfigField, HomeField};
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    // Focus a non-text field so Right switches tabs rather than moving the caret.
    app.home.focus = HomeField::NoVideo;
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

// ── footer hint count & content per context ──────────────────────────────────

/// Returns the content of the last rendered row (footer area).
fn render_footer_row(app: &App, width: u16, height: u16) -> String {
    let buf = render_to_buffer(app, width, height);
    let last_row = (height - 1) as usize;
    buf.content()
        .iter()
        .skip(last_row * width as usize)
        .take(width as usize)
        .map(|c| c.symbol())
        .collect()
}

fn hint_count(footer: &str) -> usize {
    // hints are separated by "  │  " in the rendered footer; count separators + 1
    footer.matches('│').count() + 1
}

#[test]
fn home_footer_toggle_focus_has_quit_hint_ending_with_help() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::AutoOverwrite;
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains("↑↓"), "must show move hint");
    assert!(footer.contains("enter toggle"), "must show enter toggle");
    assert!(footer.contains("q quit"), "must show q quit");
    assert!(footer.contains('?'), "must end with ? help");
    assert_eq!(
        hint_count(&footer),
        4,
        "toggle focus must show move, toggle, quit, help"
    );
}

#[test]
fn home_footer_button_focus_shows_enter_download() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::Download;
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains("↑↓"), "must show move hint");
    assert!(
        footer.contains("enter download"),
        "must show enter download"
    );
    assert!(footer.contains("q quit"), "must show q quit");
    assert!(footer.contains('?'), "must end with ? help");
    assert_eq!(
        hint_count(&footer),
        4,
        "button focus must show move, download, quit, help"
    );
}

#[test]
fn home_footer_text_input_focus_has_three_hints_with_quit() {
    use osu_collect::app::HomeField;

    let mut app = make_app();
    app.home.focus = HomeField::Collection;
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains("↑↓"), "must show move hint");
    assert!(footer.contains('q'), "must show q quit");
    assert!(footer.contains('?'), "must show ? help");
    assert_eq!(
        hint_count(&footer),
        3,
        "text input focus must show exactly 3 hints"
    );
}

#[test]
fn updates_footer_not_in_list_has_at_most_four_hints() {
    let mut app = make_app();
    app.next_tab();
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains("↑↓"), "must show move hint");
    assert!(footer.contains('?'), "must show ? help");
    assert!(
        hint_count(&footer) <= 4,
        "updates not-in-list footer must show at most 4 hints, got {}",
        hint_count(&footer)
    );
}

#[test]
fn updates_footer_in_list_has_exactly_four_hints() {
    let mut app = make_app();
    app.next_tab();
    app.updates.selection.in_collection_list = true;
    let footer = render_footer_row(&app, 200, 24);
    assert_eq!(
        hint_count(&footer),
        4,
        "updates in-list footer must show exactly 4 hints"
    );
}

#[test]
fn config_footer_non_text_has_four_hints_with_help() {
    use osu_collect::app::ConfigField;
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadNoVideo;
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains("enter toggle"), "must show enter toggle");
    assert!(footer.contains('s'), "must show s save");
    assert!(footer.contains('?'), "must show ? help");
    assert_eq!(
        hint_count(&footer),
        4,
        "config non-text footer must show exactly 4 hints"
    );
}

#[test]
fn config_footer_text_input_shows_esc_back_not_space_toggle() {
    use osu_collect::app::ConfigField;
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::MirrorCustomUrl;
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains("esc"), "text field must show esc back");
    assert!(footer.contains('?'), "text field must show ? help");
    assert!(
        !footer.contains("enter toggle"),
        "text field must not show enter toggle"
    );
    assert_eq!(
        hint_count(&footer),
        4,
        "config text field footer must show exactly 4 hints"
    );
}

#[test]
fn download_tab_footer_shows_help_hint() {
    use osu_collect::app::CollectionPage;

    let mut app = make_app();
    let page = CollectionPage::new(1, "test".to_string(), 1);
    app.downloads.push(page);
    app.active_tab = 3;
    let footer = render_footer_row(&app, 200, 24);
    assert!(footer.contains('?'), "download tab footer must show ? help");
    assert!(
        footer.contains("scroll"),
        "download tab must show scroll hint"
    );
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

// ── help overlay render ───────────────────────────────────────────────────────

#[test]
fn help_overlay_renders_keybindings_heading() {
    let mut app = make_app();
    app.help_open = true;
    let content = render_content(&app, 120, 40);
    assert!(
        content.contains("KEYBINDINGS") || content.contains("keybindings"),
        "help overlay must render a KEYBINDINGS heading"
    );
}

#[test]
fn help_overlay_contains_question_mark_entry() {
    let mut app = make_app();
    app.help_open = true;
    let content = render_content(&app, 120, 40);
    assert!(content.contains('?'), "help overlay must show ? key");
}

#[test]
fn help_overlay_hidden_when_closed() {
    let app = make_app();
    // help_open defaults to false
    let content = render_content(&app, 120, 40);
    assert!(
        !content.contains("KEYBINDINGS"),
        "KEYBINDINGS heading must not appear when help is closed"
    );
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
