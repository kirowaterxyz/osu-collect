use osu_collect::{
    app::{App, CollectionPage, ConfigField},
    config::{Config, constants::CONFIG_TAB_INDEX},
    download::{DownloadStage, DownloadSummary},
    tui,
};
use ratatui::{Terminal, backend::TestBackend, style::Color};

fn render_buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| tui::draw(frame, app))
        .expect("app should render");
    terminal.backend().buffer().clone()
}

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

    assert!(output.contains("home"));
    assert!(
        !output.contains("[ home ]"),
        "active tab must not use brackets"
    );
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

    let output = render_app(&app, 40, 12);

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
    app.config.focus = ConfigField::LoginEntry;

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
fn active_tab_has_orange_color_no_brackets_and_plain_bg() {
    let app = App::new(Config::default());
    let buf = render_buffer(&app, 80, 24);
    let orange = Color::Rgb(217, 119, 87);
    let bg = Color::Rgb(30, 30, 46);

    // active tab text ("home") must be orange
    let has_orange_h = buf
        .content
        .iter()
        .any(|cell| cell.symbol() == "h" && cell.style().fg == Some(orange));
    assert!(has_orange_h, "active tab 'home' must render with orange fg");

    // no bracket characters should appear in the header row (row 0)
    let header_row: String = buf
        .content
        .iter()
        .take(80)
        .map(|cell| cell.symbol())
        .collect();
    assert!(
        !header_row.contains('[') && !header_row.contains(']'),
        "header row must not contain bracket markers"
    );

    // header area (row 0) must use plain BG, not BG_RAISED
    let raised_bg = Color::Rgb(24, 24, 37);
    let header_has_raised = buf
        .content
        .iter()
        .take(80)
        .any(|cell| cell.style().bg == Some(raised_bg) && cell.style().bg != Some(bg));
    assert!(!header_has_raised, "header row must not use BG_RAISED fill");
}

#[test]
fn section_titles_use_orange_accent() {
    let orange = Color::Rgb(217, 119, 87);

    // home tab: COLLECTION and MIRRORS
    let app = App::new(Config::default());
    let buf = render_buffer(&app, 120, 30);
    let has_orange_c = buf
        .content
        .iter()
        .any(|cell| cell.symbol() == "C" && cell.style().fg == Some(orange));
    assert!(has_orange_c, "COLLECTION section header must be orange");

    // config tab: MIRRORS section
    let mut app2 = App::new(Config::default());
    app2.active_tab = CONFIG_TAB_INDEX;
    let buf2 = render_buffer(&app2, 120, 30);
    let has_orange_m = buf2
        .content
        .iter()
        .any(|cell| cell.symbol() == "M" && cell.style().fg == Some(orange));
    assert!(
        has_orange_m,
        "MIRRORS section header must be orange in config tab"
    );
}

#[test]
fn spinner_advances_with_tick_count() {
    use osu_collect::app::messages::AppMessage;

    let mut app = App::new(Config::default());
    app.home.message = Some(AppMessage::loading("loading..."));

    let buf0 = render_buffer(&app, 80, 24);
    // grab cells in the footer row (last row at y=23)
    let spinner0: String = buf0
        .content
        .iter()
        .skip(80 * 23)
        .take(5)
        .map(|c| c.symbol())
        .collect();

    app.tick_count = 1;
    let buf1 = render_buffer(&app, 80, 24);
    let spinner1: String = buf1
        .content
        .iter()
        .skip(80 * 23)
        .take(5)
        .map(|c| c.symbol())
        .collect();

    // the spinner character in the footer must differ between tick 0 and tick 1
    assert_ne!(spinner0, spinner1, "spinner must advance with tick_count");
}

#[test]
fn login_key_on_non_login_field_does_not_produce_login_command() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use osu_collect::app::AppCommand;

    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    // focus is MirrorNerinyan by default — not LoginAction
    let key = KeyEvent {
        code: KeyCode::Char('l'),
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };
    let cmd = app.handle_key(key);
    assert!(
        !matches!(cmd, Some(AppCommand::Login { .. })),
        "'l' on non-LoginAction focus must not trigger login"
    );
}

#[test]
fn login_field_is_reachable_via_focus_cycle() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
    use osu_collect::app::ConfigField;

    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;

    let down = KeyEvent {
        code: KeyCode::Down,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    };

    // cycle through all fields and check that LoginEntry and LogoutEntry appear
    let mut found_login = false;
    let mut found_logout = false;
    for _ in 0..25 {
        if app.config.focus == ConfigField::LoginEntry {
            found_login = true;
        }
        if app.config.focus == ConfigField::LogoutEntry {
            found_logout = true;
        }
        if found_login && found_logout {
            break;
        }
        app.handle_key(down);
    }
    assert!(
        found_login,
        "LoginEntry field must be reachable via down-arrow navigation"
    );
    assert!(
        found_logout,
        "LogoutEntry field must be reachable via down-arrow navigation"
    );
}

#[test]
fn login_action_row_renders_when_focused() {
    use osu_collect::app::ConfigField;

    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::LoginEntry;

    let output = render_app(&app, 120, 30);
    assert!(
        output.contains("log in") || output.contains("re-login") || output.contains("logging in"),
        "LoginEntry row must render a visible action label"
    );
}

#[test]
fn thread_view_renders_progress_bar_when_downloading() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 5;
    page.download_target = 5;
    page.register_beatmaps(&[42]);
    page.update_thread_status(0, "Downloading #42 from mirror", false, Some(42));
    page.update_thread_progress(0, 5_000_000, 10_000_000);
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(output.contains("█"), "filled bar segment must render");
    assert!(output.contains("░"), "empty bar segment must render");
    assert!(
        output.contains("50%"),
        "percent label must reflect progress"
    );
}

#[test]
fn thread_view_omits_progress_bar_when_fetching() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 5;
    page.download_target = 5;
    page.register_beatmaps(&[42]);
    page.update_thread_status(0, "Fetching #42 from mirror", false, Some(42));
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(output.contains("Fetching"), "fetching status must render");
    assert!(
        !output.contains("█") && !output.contains("░"),
        "bar must not render while status is not Downloading"
    );
}

#[test]
fn thread_status_change_is_debounced_except_for_downloading() {
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.update_thread_status(0, "Fetching #1 from mirror", false, Some(1));

    assert_eq!(
        page.thread_statuses[0].displayed_message(),
        "Idle",
        "non-Downloading status must wait for the debounce window"
    );

    page.update_thread_status(0, "Downloading #1 from mirror", false, Some(1));

    assert_eq!(
        page.thread_statuses[0].displayed_message(),
        "Downloading #1 from mirror",
        "Downloading status must bypass debounce and promote immediately"
    );
}

#[test]
fn fetching_status_promotes_after_debounce_expires() {
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.update_thread_status(0, "Fetching #1 from mirror", false, Some(1));

    assert_eq!(page.thread_statuses[0].displayed_message(), "Idle");

    std::thread::sleep(std::time::Duration::from_millis(150));

    assert_eq!(
        page.thread_statuses[0].displayed_message(),
        "Fetching #1 from mirror",
    );
}

#[test]
fn fetching_message_switches_active_beatmap_without_pre_emit() {
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.update_thread_status(0, "Downloading #100 from mirror", false, Some(100));
    page.update_thread_progress(0, 1_000_000, 4_000_000);

    page.update_thread_status(0, "Fetching #101 from mirror", false, Some(101));

    let thread = &page.thread_statuses[0];
    assert!(thread.message.contains("Fetching"));
    assert!(thread.message.contains("#101"));
    assert_eq!(
        thread.progress_ratio(),
        None,
        "progress must reset when switching to a new beatmap"
    );
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
