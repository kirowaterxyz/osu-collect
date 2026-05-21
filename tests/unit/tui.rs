use crate::{
    app::{App, CollectionPage, ConfigField, messages::AppMessage},
    config::{Config, constants::CONFIG_TAB_INDEX},
    download::{DownloadEvent, DownloadStage, DownloadSummary},
};
use ratatui::{Terminal, backend::TestBackend, style::Color};

fn render_buffer(app: &App, width: u16, height: u16) -> ratatui::buffer::Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| super::draw(frame, app))
        .expect("app should render");
    terminal.backend().buffer().clone()
}

fn render_app(app: &App, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| super::draw(frame, app))
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
    use crate::app::HomeField;

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
    assert!(output.contains("verify .osz integrity"));
    assert!(
        output.contains("off") && output.contains("basic") && output.contains("strict"),
        "all three archive validation labels must render: {output}"
    );
}

#[test]
fn config_render_shows_strict_help_only_when_strict_selected() {
    use crate::download::ArchiveValidation;

    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadArchiveValidation;

    app.config.archive_validation = ArchiveValidation::Magic;
    let basic = render_app(&app, 100, 24);
    assert!(
        !basic.contains("strict mode may reject"),
        "help line must be hidden when basic is selected"
    );

    app.config.archive_validation = ArchiveValidation::Eocd;
    let strict = render_app(&app, 100, 24);
    assert!(
        strict.contains("strict mode may reject"),
        "help line must appear when strict is selected: {strict}"
    );
}

#[test]
fn config_render_shows_official_login_status() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::LoginEntry;

    let output = render_app(&app, 80, 24);

    assert!(output.contains("osu! login") || output.contains("OSU! LOGIN"));
    assert!(output.contains("logged out") || output.contains("logged in"));
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
    use crate::app::messages::AppMessage;

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
    use crate::app::AppCommand;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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
    use crate::app::AuthLoginState;
    use crate::app::ConfigField;
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

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
    use crate::app::ConfigField;

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
fn active_view_renders_progress_bar_when_downloading() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 5;
    page.download_target = 5;
    page.register_beatmaps(&[42]);
    page.update_active_status(
        42,
        crate::download::BeatmapStage::Downloading,
        "Downloading #42 from mirror",
        false,
        None,
    );
    page.update_active_progress(42, 5_000_000, 10_000_000);
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
fn active_view_requires_percentage_for_discovered_download_size() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 5;
    page.download_target = 5;
    page.register_beatmaps(&[42]);
    page.update_active_status(
        42,
        crate::download::BeatmapStage::Downloading,
        "Downloading #42 from mirror",
        false,
        None,
    );
    page.update_active_progress(42, 1_500_000, 10_000_000);
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(output.contains("█"), "filled bar segment must render");
    assert!(
        output.contains("15%"),
        "percent label must reflect probed size"
    );
    assert!(!output.contains("  --"), "progress must render as percent");
}

#[test]
fn active_view_renders_bouncing_bar_when_total_is_unknown() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 5;
    page.download_target = 5;
    page.register_beatmaps(&[42]);
    page.update_active_status(
        42,
        crate::download::BeatmapStage::Downloading,
        "Downloading #42 from mirror",
        false,
        None,
    );
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(output.contains("████"), "bouncing segment must render");
    assert!(output.contains("..."), "unknown progress label must render");
    assert!(
        !output.contains("  --"),
        "unknown progress must not render --"
    );
}

#[test]
fn active_panel_height_is_constant_across_completion_and_start() {
    use crate::download::BeatmapStage;

    fn count_id_prefixes(output: &str) -> usize {
        output.matches("  #").count()
    }

    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked".into(), 3);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 10;
    page.download_target = 10;
    page.register_beatmaps(&[10, 11, 12, 13]);
    for id in [10u32, 11, 12] {
        page.update_active_status(
            id,
            BeatmapStage::Downloading,
            &format!("Downloading #{id} from mirror"),
            false,
            None,
        );
    }
    app.downloads.push(page);
    app.active_tab = 3;

    let baseline = render_app(&app, 120, 30);
    let baseline_total = app.downloads[0].thread_total_items.get();
    assert_eq!(baseline_total, 3, "concurrent=3 means 3 active rows");
    assert_eq!(count_id_prefixes(&baseline), 3);

    // complete the middle slot — total row count stays the same and the slot keeps
    // rendering its terminal message ("done") instead of going blank until the next
    // beatmapset arrives. text is debounced so we wait past the window before rendering.
    app.downloads[0].update_active_status(11, BeatmapStage::Success, "done", false, None);
    std::thread::sleep(std::time::Duration::from_millis(75));
    let after_complete = render_app(&app, 120, 30);
    assert_eq!(
        app.downloads[0].thread_total_items.get(),
        baseline_total,
        "freed slot must not collapse the panel height"
    );
    assert_eq!(
        count_id_prefixes(&after_complete),
        3,
        "terminal slot must keep rendering its row so it never flashes blank"
    );
    assert!(
        after_complete.contains("done"),
        "terminal message must remain visible until the slot is reused"
    );

    // refill — the lingering terminal slot is reused, ids stay at 3
    app.downloads[0].update_active_status(
        99,
        BeatmapStage::Downloading,
        "Downloading #99 from mirror",
        false,
        None,
    );
    let after_refill = render_app(&app, 120, 30);
    assert_eq!(app.downloads[0].thread_total_items.get(), baseline_total);
    assert_eq!(count_id_prefixes(&after_refill), 3);
    assert!(
        after_refill.contains("#99"),
        "new beatmapset must take the lingering terminal slot"
    );

    // and an all-empty active panel still keeps `concurrent` rows so the stage transition
    // from rechecking to first lib status can't flash a placeholder for a single frame
    app.downloads[0].clear_active_downloads();
    let _ = render_app(&app, 120, 30);
    assert_eq!(app.downloads[0].thread_total_items.get(), baseline_total);
}

#[test]
fn long_message_does_not_drop_the_progress_bar() {
    use crate::download::BeatmapStage;

    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked".into(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 1;
    page.download_target = 1;
    page.register_beatmaps(&[42]);
    page.update_active_status(
        42,
        BeatmapStage::Downloading,
        "retrying nerinyan-extra-long-mirror-name after Connection timed out (attempt 3/3)",
        false,
        None,
    );
    page.update_active_progress(42, 7_000_000, 10_000_000);
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 60, 24);
    assert!(
        output.contains("█") && output.contains("░"),
        "bar must remain visible even when the message would otherwise overflow"
    );
    assert!(
        output.contains("70%"),
        "percent label must still render after truncation"
    );
}

#[test]
fn active_view_shows_bar_for_active_download_regardless_of_message() {
    // The bar is keyed on the beatmap's `BeatmapStage`, not on the message string, so
    // sub-state transitions (retrying, mirror failed, verifying) keep the bar visible
    // and don't flicker on every status update.
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.stage = DownloadStage::Downloading;
    page.total_maps = 5;
    page.download_target = 5;
    page.register_beatmaps(&[42]);
    page.update_active_status(
        42,
        crate::download::BeatmapStage::Downloading,
        "retrying nerinyan after timeout (attempt 2/3)",
        false,
        None,
    );
    page.update_active_progress(42, 3_000_000, 6_000_000);
    std::thread::sleep(std::time::Duration::from_millis(150));
    app.downloads.push(page);
    app.active_tab = 3;

    let output = render_app(&app, 100, 24);

    assert!(output.contains("retrying"), "transient status must render");
    assert!(
        output.contains("█") && output.contains("░"),
        "bar must remain visible across in-flight sub-states"
    );
    assert!(
        output.contains("50%"),
        "percent label must reflect latest progress"
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
fn resolving_stage_with_progress_shows_count_in_title() {
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
fn active_progress_is_per_beatmapset() {
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 2);
    page.update_active_status(
        100,
        crate::download::BeatmapStage::Downloading,
        "Downloading #100 from mirror",
        false,
        None,
    );
    page.update_active_progress(100, 1_000_000, 4_000_000);
    page.update_active_status(
        101,
        crate::download::BeatmapStage::Downloading,
        "Fetching #101 from mirror",
        false,
        None,
    );

    let line_100 = page
        .active_lines()
        .find(|l| l.beatmapset_id == 100)
        .expect("100 still active");
    assert_eq!(line_100.downloaded, 1_000_000);

    let line_101 = page
        .active_lines()
        .find(|l| l.beatmapset_id == 101)
        .expect("101 inserted");
    assert!(line_101.displayed_message().contains("Fetching"));
    assert_eq!(line_101.progress_ratio(), None);
}

#[test]
fn precheck_pending_status_does_not_consume_active_slot() {
    use crate::download::BeatmapStage;

    let mut page = CollectionPage::new(1, "ranked".into(), 2);
    // a flood of precheck "file changed" notifications must not pile up in the active panel
    for id in 0u32..50 {
        page.update_active_status(
            id,
            BeatmapStage::Pending,
            "file changed during precheck; re-downloading",
            false,
            None,
        );
    }
    assert_eq!(
        page.active_lines().count(),
        0,
        "precheck Pending events must not allocate active slots"
    );

    // a real download then claims a slot
    page.update_active_status(
        7,
        BeatmapStage::Downloading,
        "Downloading #7 from mirror",
        false,
        None,
    );
    assert_eq!(page.active_lines().count(), 1);
}

#[test]
fn active_slot_count_is_capped_at_thread_count() {
    use crate::download::BeatmapStage;

    let mut page = CollectionPage::new(1, "ranked".into(), 2);
    for id in [10u32, 11, 12, 13] {
        page.update_active_status(
            id,
            BeatmapStage::Downloading,
            &format!("Downloading #{id}"),
            false,
            None,
        );
    }
    assert_eq!(
        page.active_lines().count(),
        2,
        "active slots must not exceed concurrent thread count"
    );

    // when one terminates the freed slot can be reused
    page.update_active_status(10, BeatmapStage::Success, "done", false, None);
    page.update_active_status(
        12,
        BeatmapStage::Downloading,
        "Downloading #12",
        false,
        None,
    );
    let ids: std::collections::BTreeSet<u32> =
        page.active_lines().map(|l| l.beatmapset_id).collect();
    assert_eq!(ids, [11, 12].into_iter().collect());
}

#[test]
fn freed_slot_position_is_reused_for_stability() {
    use crate::download::BeatmapStage;

    let mut page = CollectionPage::new(1, "ranked".into(), 3);
    for id in [20u32, 21, 22] {
        page.update_active_status(
            id,
            BeatmapStage::Downloading,
            &format!("Downloading #{id}"),
            false,
            None,
        );
    }
    let position_of = |page: &CollectionPage, target: u32| -> Option<usize> {
        page.active_downloads.iter().position(|slot| {
            slot.as_ref()
                .is_some_and(|line| line.beatmapset_id == target)
        })
    };
    let pos_22 = position_of(&page, 22).expect("22 placed");

    // the middle slot frees; a new download must take that exact slot so the bottom row
    // doesn't shift visually.
    page.update_active_status(21, BeatmapStage::Success, "done", false, None);
    page.update_active_status(
        99,
        BeatmapStage::Downloading,
        "Downloading #99",
        false,
        None,
    );

    assert_eq!(position_of(&page, 99), Some(1));
    assert_eq!(
        position_of(&page, 22),
        Some(pos_22),
        "untouched neighbours must keep their slot index"
    );
}

#[test]
fn progress_alone_must_not_allocate_an_empty_slot() {
    use crate::download::BeatmapStage;

    let mut page = CollectionPage::new(1, "ranked".into(), 2);
    // a progress event without a preceding status event must not create a blank-message
    // slot — the lib always emits Contacting/Downloading first, and creating a line with
    // empty `displayed_message` is exactly the flicker source we're avoiding
    page.update_active_progress(42, 1_024, 4_096);
    assert_eq!(page.active_lines().count(), 0);

    // once the status event lands the slot allocates with a real message; subsequent
    // progress updates land on the same slot.
    page.update_active_status(
        42,
        BeatmapStage::Downloading,
        "contacting nerinyan",
        false,
        None,
    );
    page.update_active_progress(42, 1_024, 4_096);
    let line = page
        .active_lines()
        .find(|l| l.beatmapset_id == 42)
        .expect("slot held");
    assert!(!line.displayed_message().is_empty());
    assert_eq!(line.downloaded, 1_024);
}

#[test]
fn bar_visible_during_downloading_before_bytes_flow() {
    use crate::download::BeatmapStage;

    use super::accent;

    let mut page = CollectionPage::new(1, "ranked".into(), 1);
    page.update_active_status(
        7,
        BeatmapStage::Downloading,
        "contacting nerinyan",
        false,
        None,
    );
    let line = page.active_lines().next().expect("slot allocated");
    assert_eq!(
        line.bar_color(),
        accent(),
        "active downloads without a total should show an indeterminate bar in accent color"
    );

    page.update_active_progress(7, 4_096, 8_192);
    let line = page.active_lines().next().expect("slot allocated");
    assert_eq!(
        line.bar_color(),
        accent(),
        "bar must remain in accent color once real progress data is available"
    );
}

#[test]
fn first_status_lands_immediately_then_text_is_debounced() {
    use crate::download::BeatmapStage;

    let mut page = CollectionPage::new(1, "x".into(), 1);
    page.update_active_status(
        200,
        BeatmapStage::Downloading,
        "Downloading #200 ...",
        false,
        None,
    );
    let initial = page.active_downloads[0]
        .as_ref()
        .expect("slot must be allocated")
        .displayed_message();
    assert!(
        initial.starts_with("Downloading"),
        "first write must land instantly so the slot isn't blank, got {initial:?}"
    );

    // second write within the debounce window stays queued. after the window expires,
    // displayed flips to the latest pending value on next read.
    page.update_active_status(
        200,
        BeatmapStage::Downloading,
        "Rate limited on X, ...",
        true,
        None,
    );
    std::thread::sleep(std::time::Duration::from_millis(75));
    let line = page.active_downloads[0]
        .as_ref()
        .expect("slot must be allocated");
    let visible = line.displayed_message();
    assert!(
        visible.starts_with("Rate limited"),
        "queued status must surface after the debounce window, got {visible:?}"
    );
    assert!(line.displayed_rate_limited());

    page.update_active_status(200, BeatmapStage::Downloading, "", false, None);
    std::thread::sleep(std::time::Duration::from_millis(75));
    let fallback = page.active_downloads[0]
        .as_ref()
        .expect("slot must be allocated")
        .displayed_message();
    assert_eq!(fallback, "downloading #200");
}

#[test]
fn rapid_status_transitions_coalesce_to_latest() {
    use crate::download::BeatmapStage;

    let mut page = CollectionPage::new(1, "x".into(), 1);
    page.update_active_status(
        400,
        BeatmapStage::Downloading,
        "downloading from nerinyan",
        false,
        None,
    );
    page.update_active_status(
        400,
        BeatmapStage::Downloading,
        "checking nerinyan",
        false,
        None,
    );
    page.update_active_status(
        400,
        BeatmapStage::Downloading,
        "rate limited on nerinyan, waiting 5s",
        true,
        None,
    );
    std::thread::sleep(std::time::Duration::from_millis(75));

    let line = page.active_downloads[0]
        .as_ref()
        .expect("slot must be allocated");
    let visible = line.displayed_message();
    assert!(
        visible.starts_with("rate limited"),
        "intermediate texts must coalesce; only the final state shows after the window, got {visible:?}"
    );
    assert!(line.displayed_rate_limited());
}

#[test]
fn terminal_stage_clears_active_downloads() {
    let mut app = App::new(Config::default());
    let mut page = CollectionPage::new(1, "ranked maps".to_string(), 1);
    page.update_active_status(
        100,
        crate::download::BeatmapStage::Downloading,
        "Downloading #100",
        false,
        None,
    );
    app.downloads.push(page);

    app.handle_download_event(DownloadEvent::Failed {
        id: 1,
        message: "boom".into(),
    });
    assert_eq!(
        app.downloads[0].active_lines().count(),
        0,
        "active_downloads must be cleared on Failed"
    );

    app.downloads[0].update_active_status(
        101,
        crate::download::BeatmapStage::Downloading,
        "Downloading #101",
        false,
        None,
    );
    app.handle_download_event(DownloadEvent::StageChanged {
        id: 1,
        stage: DownloadStage::Completed,
    });
    assert_eq!(
        app.downloads[0].active_lines().count(),
        0,
        "active_downloads must be cleared on StageChanged Completed"
    );
}

#[test]
fn footer_info_message_uses_info_color() {
    let mut app = App::new(Config::default());
    app.home.message = Some(AppMessage::info("ready"));
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| super::draw(frame, &app))
        .expect("app should render");

    let has_info_cell = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .any(|cell| cell.symbol() == "i" && cell.style().fg == Some(Color::Rgb(116, 199, 236)));

    assert!(has_info_cell);
}

#[test]
fn threads_stepper_renders_recommended_chip_when_value_differs() {
    use crate::app::HomeField;

    let mut app = App::new(Config::default());
    app.home.focus = HomeField::Threads;
    // Set to something other than the recommended (default_threads).
    let recommended = app.home.default_threads;
    let non_default = if recommended > 1 { recommended - 1 } else { 2 };
    app.home.threads.value = non_default.to_string();

    let output = render_app(&app, 80, 30);

    assert!(
        output.contains("recommended"),
        "recommended chip must appear when thread count differs from CPU count: {output}"
    );
}

#[test]
fn threads_stepper_omits_recommended_chip_when_at_default() {
    use crate::app::HomeField;

    let mut app = App::new(Config::default());
    app.home.focus = HomeField::Threads;
    // Set value to exactly the recommended amount.
    let recommended = app.home.default_threads;
    app.home.threads.value = recommended.to_string();

    let output = render_app(&app, 80, 30);

    assert!(
        !output.contains("recommended"),
        "recommended chip must be omitted when value equals CPU count"
    );
}

#[test]
fn home_hint_shows_plus_minus_when_threads_focused() {
    use crate::app::HomeField;

    let mut app = App::new(Config::default());
    app.home.focus = HomeField::Threads;

    let output = render_app(&app, 80, 24);

    assert!(
        output.contains("+/-") || output.contains("+"),
        "footer must show +/- hint when threads stepper is focused: {output}"
    );
}

#[test]
fn config_archive_validation_help_shows_when_focused() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadArchiveValidation;

    let output = render_app(&app, 100, 30);

    assert!(
        output.contains("basic verifies headers"),
        "archive validation help must appear when field is focused: {output}"
    );
}

#[test]
fn config_archive_validation_help_hidden_when_not_focused() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadThreads;

    let output = render_app(&app, 100, 30);

    assert!(
        !output.contains("basic verifies headers"),
        "archive validation help must not appear when field is not focused: {output}"
    );
    assert!(
        !output.contains("strict mode may reject"),
        "strict help must not appear when field is not focused: {output}"
    );
}

#[test]
fn config_archive_validation_strict_help_when_strict_selected_and_focused() {
    use crate::download::ArchiveValidation;

    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadArchiveValidation;
    app.config.archive_validation = ArchiveValidation::Eocd;

    let output = render_app(&app, 100, 30);

    assert!(
        output.contains("strict mode may reject"),
        "strict help must appear when strict is selected and field is focused: {output}"
    );
    assert!(
        !output.contains("off skips checks"),
        "generic help must not appear alongside the strict help: {output}"
    );
}

#[test]
fn config_custom_mirror_help_shows_when_focused() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::MirrorCustomUrl;

    let output = render_app(&app, 100, 30);

    assert!(
        output.contains("must contain {id}"),
        "custom mirror help must appear when field is focused: {output}"
    );
}

#[test]
fn config_custom_mirror_help_hidden_when_not_focused() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.config.focus = ConfigField::DownloadThreads;

    let output = render_app(&app, 100, 30);

    assert!(
        !output.contains("must contain {id}"),
        "custom mirror help must not appear when field is not focused: {output}"
    );
}

#[test]
fn home_custom_mirror_help_shows_when_focused() {
    use crate::app::HomeField;

    let mut app = App::new(Config::default());
    app.home.focus = HomeField::CustomMirror;

    let output = render_app(&app, 100, 30);

    assert!(
        output.contains("must contain {id}"),
        "custom mirror help must appear when field is focused: {output}"
    );
}

#[test]
fn home_custom_mirror_help_hidden_when_not_focused() {
    use crate::app::HomeField;

    let mut app = App::new(Config::default());
    app.home.focus = HomeField::MirrorNerinyan;

    let output = render_app(&app, 100, 30);

    assert!(
        !output.contains("must contain {id}"),
        "custom mirror help must not appear when field is not focused: {output}"
    );
}

#[test]
fn updates_osu_path_help_shows_when_focused() {
    use crate::app::UpdatesField;
    use crate::config::constants::UPDATES_TAB_INDEX;

    let mut app = App::new(Config::default());
    app.active_tab = UPDATES_TAB_INDEX;
    app.updates.selection.focus = UpdatesField::OsuPath;

    let output = render_app(&app, 100, 30);

    assert!(
        output.contains("must contain osu!.db"),
        "osu! path help must appear when field is focused: {output}"
    );
}

#[test]
fn updates_osu_path_help_hidden_when_not_focused() {
    use crate::app::UpdatesField;
    use crate::config::constants::UPDATES_TAB_INDEX;

    let mut app = App::new(Config::default());
    app.active_tab = UPDATES_TAB_INDEX;
    app.updates.selection.focus = UpdatesField::ClientType;

    let output = render_app(&app, 100, 30);

    assert!(
        !output.contains("must contain osu!.db"),
        "osu! path help must not appear when field is not focused: {output}"
    );
}

// --- status pill tests ---

#[test]
fn pill_shows_downloading_count_when_at_least_one_page_is_downloading() {
    use super::header::StatusPill;

    let pill = StatusPill::compute(2, None).expect("two downloading pages produces a pill");
    let segs = pill.segments();
    let all_text: String = segs.iter().map(|(t, _)| t.as_str()).collect();
    assert!(
        all_text.contains("2 downloading"),
        "pill must display downloading count, got {all_text:?}"
    );
}

#[test]
fn pill_omits_downloading_segment_when_count_is_zero() {
    use super::header::StatusPill;

    // disk_free is large enough to not be None
    let pill = StatusPill::compute(0, Some(10 * 1024 * 1024 * 1024));
    let pill = pill.expect("disk segment keeps pill visible");
    let segs = pill.segments();
    let all_text: String = segs.iter().map(|(t, _)| t.as_str()).collect();
    assert!(
        !all_text.contains("downloading"),
        "downloading segment must be absent when count is 0, got {all_text:?}"
    );
}

#[test]
fn pill_is_none_when_no_downloads_and_no_disk_path() {
    use super::header::StatusPill;

    assert!(
        StatusPill::compute(0, None).is_none(),
        "pill must be None when nothing to show"
    );
}

#[test]
fn pill_disk_color_is_dim_above_warn_threshold() {
    use super::header::StatusPill;
    use super::text_dim;

    // 2 GiB — well above 1 GiB warn threshold
    let free = 2u64 * 1024 * 1024 * 1024;
    let pill = StatusPill::compute(0, Some(free)).expect("disk segment present");
    let segs = pill.segments();
    let disk_seg = segs
        .iter()
        .find(|(t, _)| t.contains("free"))
        .expect("disk segment");
    assert_eq!(
        disk_seg.1,
        text_dim(),
        "disk segment must use text_dim when free space is healthy"
    );
}

#[test]
fn pill_disk_color_is_warning_below_1_gib() {
    use super::header::StatusPill;
    use super::warning;

    // 500 MiB — below 1 GiB warn threshold, above 100 MiB danger threshold
    let free = 500u64 * 1024 * 1024;
    let pill = StatusPill::compute(0, Some(free)).expect("disk segment present");
    let segs = pill.segments();
    let disk_seg = segs
        .iter()
        .find(|(t, _)| t.contains("free"))
        .expect("disk segment");
    assert_eq!(
        disk_seg.1,
        warning(),
        "disk segment must use warning color when free space is below 1 GiB"
    );
}

#[test]
fn pill_disk_color_is_danger_below_100_mib() {
    use super::danger;
    use super::header::StatusPill;

    // 50 MiB — below 100 MiB danger threshold
    let free = 50u64 * 1024 * 1024;
    let pill = StatusPill::compute(0, Some(free)).expect("disk segment present");
    let segs = pill.segments();
    let disk_seg = segs
        .iter()
        .find(|(t, _)| t.contains("free"))
        .expect("disk segment");
    assert_eq!(
        disk_seg.1,
        danger(),
        "disk segment must use danger color when free space is below 100 MiB"
    );
}

#[test]
fn header_renders_brand_tabs_and_version_regions() {
    let app = App::new(Config::default());
    let output = render_app(&app, 120, 24);

    assert!(
        output.contains("osu-collect"),
        "brand must render in header"
    );
    assert!(output.contains("home"), "tabs must render in header");
    let version = concat!("v", env!("CARGO_PKG_VERSION"));
    assert!(
        output.contains(version),
        "version must render in header: expected {version:?} in output"
    );
}

#[test]
fn downloading_count_reflects_downloading_stage_pages_only() {
    let mut app = App::new(Config::default());

    let mut page1 = CollectionPage::new(1, "a".into(), 1);
    page1.stage = DownloadStage::Downloading;

    let mut page2 = CollectionPage::new(2, "b".into(), 1);
    page2.stage = DownloadStage::Completed;

    let mut page3 = CollectionPage::new(3, "c".into(), 1);
    page3.stage = DownloadStage::Downloading;

    app.downloads.push(page1);
    app.downloads.push(page2);
    app.downloads.push(page3);

    assert_eq!(
        app.downloading_count(),
        2,
        "only Downloading-stage pages should be counted"
    );
}
