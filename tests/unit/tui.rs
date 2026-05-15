use osu_collect::{
    app::{App, CollectionPage, ConfigField},
    config::{Config, constants::CONFIG_TAB_INDEX},
    download::{DownloadEvent, DownloadStage, DownloadSummary},
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

fn progress_fill_positions(buf: &ratatui::buffer::Buffer, color: Color) -> Vec<(u16, u16)> {
    buf.content
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.symbol() == "█" && cell.style().fg == Some(color))
        .map(|(i, _)| {
            let x = (i as u16) % buf.area.width;
            let y = (i as u16) / buf.area.width;
            (x, y)
        })
        .collect()
}

#[test]
fn home_render_shows_cloudy_sections_and_footer() {
    use osu_collect::app::HomeField;

    let mut app = App::new(Config::default());
    // focus a mirror toggle so the footer hint exposes the space shortcut
    app.home.focus = HomeField::MirrorNerinyan;

    let output = render_app(&app, 80, 24);

    assert!(output.contains("home"));
    assert!(
        !output.contains("[ home ]"),
        "active tab must not use brackets"
    );
    assert!(output.contains("COLLECTION"));
    assert!(output.contains("MIRRORS"));
    assert!(output.contains("DOWNLOAD"));
    assert!(output.contains("space"));
    assert!(output.contains("enter"));
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

    assert!(output.contains("DOWNLOAD"));
    assert!(output.contains("skip existing files"));
    assert!(output.contains("verify .osz integrity"));
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

    use osu_collect::app::AuthLoginState;

    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    // logout entry only appears when logged in
    app.config.auth_loaded = true;
    app.config.login_state = AuthLoginState::LoggedIn;

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
fn rechecking_stage_shows_verification_progress() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Rechecking;
    page.total_maps = 10;
    page.download_target = 10;
    page.stats.skipped = 3;
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(
        output.contains("3/10 verified"),
        "gauge must show running verification count during rechecking"
    );
}

#[test]
fn rechecking_stage_replaces_top_title_with_recheck_progress() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Rechecking;
    page.total_maps = 10;
    page.download_target = 10;
    page.stats.skipped = 3;
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(
        output.contains("rechecking 3/10"),
        "gauge top title must show recheck progress instead of downloaded/queued during rechecking"
    );
    assert!(
        !output.contains("queued"),
        "queued count must not appear in the gauge top title during rechecking"
    );
}

#[test]
fn rechecking_stage_threads_panel_shows_verification_status() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Rechecking;
    page.total_maps = 10;
    page.download_target = 10;
    page.stats.skipped = 2;
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(
        output.contains("verifying existing archives"),
        "threads panel must show verification status during rechecking"
    );
    assert!(
        !output.contains("no active threads"),
        "rechecking must replace the idle-threads placeholder"
    );
}

#[test]
fn resolving_stage_renders_indeterminate_gauge_and_status() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Resolving;
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(
        output.contains("resolving collection"),
        "gauge title must announce the resolving stage"
    );
    assert!(
        output.contains("fetching collection metadata"),
        "threads panel must replace the idle placeholder while resolving"
    );
    assert!(
        !output.contains("0 downloaded  0 queued"),
        "downloaded/queued label must not appear while resolving"
    );
    assert!(
        !output.contains("0/1 verified"),
        "verified label must not appear while resolving"
    );
}

#[test]
fn resolving_stage_with_progress_shows_count_in_title_and_threads() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Resolving;
    page.resolve_progress = Some((2, 5));
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(
        output.contains("resolving 2/5 collections"),
        "gauge title must reflect resolve progress"
    );
    assert!(
        output.contains("fetching collection 2/5"),
        "threads panel must show resolve progress count"
    );
}

#[test]
fn resolving_stage_indeterminate_chunk_starts_at_one_third() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Resolving;
    app.downloads.push(page);
    app.active_tab = 3;
    app.tick_count = 47;

    let buf = render_buffer(&app, 100, 24);
    let cyan = Color::Rgb(116, 199, 236);
    let positions = progress_fill_positions(&buf, cyan);
    let start_x = positions
        .iter()
        .map(|(x, _)| *x)
        .min()
        .expect("indeterminate chunk must render");

    assert_eq!(start_x, 33, "indeterminate chunk must start at 1/3");
}

#[test]
fn resolving_stage_indeterminate_chunk_advances_with_tick() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Resolving;
    app.downloads.push(page);
    app.active_tab = 3;

    let buf0 = render_buffer(&app, 100, 24);
    app.tick_count = 4;
    let buf1 = render_buffer(&app, 100, 24);

    let cyan = Color::Rgb(116, 199, 236);
    let positions0 = progress_fill_positions(&buf0, cyan);
    let positions1 = progress_fill_positions(&buf1, cyan);

    assert!(!positions0.is_empty(), "indeterminate chunk must render");
    assert_ne!(
        positions0, positions1,
        "indeterminate chunk must move with tick_count"
    );
}

#[test]
fn stage_change_resets_indeterminate_chunk_start() {
    let mut app = App::new(Config::default());
    let page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    app.downloads.push(page);
    app.active_tab = 3;
    app.tick_count = 7;
    app.handle_download_event(DownloadEvent::StageChanged {
        id: 1,
        stage: DownloadStage::Resolving,
    });
    let first = render_buffer(&app, 100, 24);

    app.tick_count = 19;
    app.handle_download_event(DownloadEvent::StageChanged {
        id: 1,
        stage: DownloadStage::Downloading,
    });
    app.handle_download_event(DownloadEvent::StageChanged {
        id: 1,
        stage: DownloadStage::Resolving,
    });
    let second = render_buffer(&app, 100, 24);

    let cyan = Color::Rgb(116, 199, 236);

    assert_eq!(
        progress_fill_positions(&first, cyan),
        progress_fill_positions(&second, cyan),
        "resolving animation must restart after leaving the stage"
    );
}

#[test]
fn resolving_stage_progress_bar_renders_single_row() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Resolving;
    page.resolve_progress = Some((2, 5));
    app.downloads.push(page);
    app.active_tab = 3;

    let buf = render_buffer(&app, 100, 24);
    let info = Color::Rgb(116, 199, 236);

    let rows_with_info_fill: std::collections::BTreeSet<u16> = buf
        .content
        .iter()
        .enumerate()
        .filter(|(_, cell)| cell.symbol() == "█" && cell.style().fg == Some(info))
        .map(|(i, _)| (i as u16) / buf.area.width)
        .collect();

    assert_eq!(
        rows_with_info_fill.len(),
        1,
        "resolving bar must be exactly 1 row thick, got rows {rows_with_info_fill:?}"
    );
}

#[test]
fn rechecking_stage_uses_warning_color_on_gauge() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 4);
    page.stage = DownloadStage::Rechecking;
    page.total_maps = 10;
    page.download_target = 10;
    page.stats.skipped = 5;
    app.downloads.push(page);
    app.active_tab = 3;

    let buf = render_buffer(&app, 100, 24);
    let warning = Color::Rgb(249, 226, 175);

    let has_warning_fill = buf
        .content
        .iter()
        .any(|cell| cell.symbol() == "█" && cell.style().fg == Some(warning));
    assert!(
        has_warning_fill,
        "gauge fill must use warning color during rechecking"
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
