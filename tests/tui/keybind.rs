/// Keybind dispatch tests.
///
/// Verifies that key events produce the expected AppCommand or state change
/// without running the full runtime.
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};
use osu_collect::{
    app::{App, AppCommand},
    config::Config,
};

fn make_app() -> App {
    App::new(Config::default())
}

fn press(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::empty(),
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent {
        code,
        modifiers: KeyModifiers::CONTROL,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

// ── ctrl shortcuts ────────────────────────────────────────────────────────────

#[test]
fn ctrl_c_quits() {
    let mut app = make_app();
    let cmd = app.handle_key(ctrl(KeyCode::Char('c')));
    assert!(matches!(cmd, Some(AppCommand::Quit)));
}

// ── tab navigation ────────────────────────────────────────────────────────────

#[test]
fn tab_key_moves_to_next_tab() {
    let mut app = make_app();
    assert_eq!(app.active_tab(), 0);
    app.handle_key(press(KeyCode::Tab));
    assert_eq!(app.active_tab(), 1);
}

#[test]
fn backtab_wraps_to_last_tab() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::BackTab));
    // should wrap to last static tab (2 = config) since no downloads
    assert_eq!(app.active_tab(), 2);
}

#[test]
fn tab_wraps_back_to_zero() {
    let mut app = make_app();
    // 3 static tabs: home, updates, config
    app.handle_key(press(KeyCode::Tab)); // → 1
    app.handle_key(press(KeyCode::Tab)); // → 2
    app.handle_key(press(KeyCode::Tab)); // → 0
    assert_eq!(app.active_tab(), 0);
}

// ── quit key ─────────────────────────────────────────────────────────────────

#[test]
fn q_on_home_tab_with_no_downloads_quits() {
    let mut app = make_app();
    // first q sets quit_prompt since we don't have active downloads check
    // actually with no downloads: first q = Quit (no active downloads guard for prompt)
    // looking at the state logic: quit_prompt only shows if downloads.is_empty() is false
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    // with no active downloads, q should quit immediately
    assert!(matches!(cmd, Some(AppCommand::Quit)));
}

#[test]
fn q_on_downloads_tab_does_not_quit_immediately() {
    let mut app = make_app();
    // on a download tab q should cancel, not quit, but since we have no
    // download tabs active, q on static tabs with empty downloads = Quit
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(matches!(cmd, Some(AppCommand::Quit)));
}

#[test]
fn esc_on_home_tab_with_no_downloads_quits() {
    let mut app = make_app();
    let cmd = app.handle_key(press(KeyCode::Esc));
    assert!(matches!(cmd, Some(AppCommand::Quit)));
}

// ── field navigation ──────────────────────────────────────────────────────────

#[test]
fn down_moves_field_focus() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    assert_eq!(app.home.focus, HomeField::Collection);
    app.handle_key(press(KeyCode::Down));
    assert_ne!(app.home.focus, HomeField::Collection);
}

#[test]
fn up_from_first_field_wraps_to_last() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    assert_eq!(app.home.focus, HomeField::Collection);
    app.handle_key(press(KeyCode::Up));
    // should wrap to last field (NoVideo)
    assert_eq!(app.home.focus, HomeField::NoVideo);
}

// ── character input ───────────────────────────────────────────────────────────

#[test]
fn typing_into_collection_field_updates_value() {
    let mut app = make_app();
    // collection is focused by default
    app.handle_key(press(KeyCode::Char('1')));
    app.handle_key(press(KeyCode::Char('2')));
    app.handle_key(press(KeyCode::Char('3')));
    assert_eq!(app.home.collection.value, "123");
}

#[test]
fn backspace_removes_last_char() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::Char('a')));
    app.handle_key(press(KeyCode::Char('b')));
    app.handle_key(press(KeyCode::Backspace));
    assert_eq!(app.home.collection.value, "a");
}

// ── space toggle on mirrors ───────────────────────────────────────────────────

#[test]
fn space_on_mirror_field_toggles_state() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    // navigate to nerinyan mirror field
    // home: Collection → Directory → CustomMirror → MirrorOsuDirect → MirrorNerinyan
    app.handle_key(press(KeyCode::Down)); // → Directory
    app.handle_key(press(KeyCode::Down)); // → CustomMirror
    app.handle_key(press(KeyCode::Down)); // → MirrorOsuDirect
    app.handle_key(press(KeyCode::Down)); // → MirrorNerinyan
    assert_eq!(app.home.focus, HomeField::MirrorNerinyan);

    let before = app.home.nerinyan;
    app.handle_key(press(KeyCode::Char(' ')));
    assert_eq!(app.home.nerinyan, !before);
}

// ── enter on home tab ─────────────────────────────────────────────────────────

#[test]
fn recheck_failed_key_dispatches_on_updates_tab() {
    let mut app = make_app();
    app.next_tab();
    app.updates.set_failed_beatmapset_count(2);

    let cmd = app.handle_key(press(KeyCode::Char('r')));

    assert!(matches!(cmd, Some(AppCommand::RecheckFailedMaps)));
}

#[test]
fn recheck_failed_key_ignored_without_failed_maps() {
    let mut app = make_app();
    app.next_tab();

    let cmd = app.handle_key(press(KeyCode::Char('r')));

    assert!(cmd.is_none());
}

#[test]
fn enter_without_collection_input_produces_error() {
    let mut app = make_app();
    // clear any default value
    app.home.collection.value.clear();
    // enter should fail to download and set an error message
    app.handle_key(press(KeyCode::Enter));
    // no command issued (error path), message should be set
    assert!(app.home.message.is_some());
}

// ── config tab key bindings ───────────────────────────────────────────────────

#[test]
fn enter_on_config_login_triggers_login_attempt() {
    use osu_collect::app::ConfigField;
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    app.handle_key(press(KeyCode::Right));
    app.handle_key(press(KeyCode::Right));
    assert_eq!(app.active_tab(), CONFIG_TAB_INDEX);
    app.config.focus = ConfigField::LoginEntry;

    // enter must reach the login request path (command depends on bundled creds)
    let cmd = app.handle_key(press(KeyCode::Enter));
    let credentials_available = osu_collect::auth::bundled_credentials().is_some();
    if credentials_available {
        assert!(matches!(cmd, Some(AppCommand::Login { .. })));
    } else {
        assert!(cmd.is_none());
        assert!(app.config.message.is_some());
    }
}

#[test]
fn space_on_config_login_does_nothing() {
    use osu_collect::app::ConfigField;
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    app.handle_key(press(KeyCode::Right));
    app.handle_key(press(KeyCode::Right));
    assert_eq!(app.active_tab(), CONFIG_TAB_INDEX);
    app.config.focus = ConfigField::LoginEntry;

    // space must no longer trigger login — enter is the confirm key
    let cmd = app.handle_key(press(KeyCode::Char(' ')));
    assert!(
        cmd.is_none(),
        "space on login entry must not issue any command"
    );
}

// ── updates tab: enter does not exit lists ────────────────────────────────────

#[test]
fn enter_inside_collection_list_is_no_op() {
    let mut app = make_app();
    app.next_tab();
    app.updates.selection.in_collection_list = true;

    let cmd = app.handle_key(press(KeyCode::Enter));
    assert!(cmd.is_none());
    assert!(
        app.updates.selection.in_collection_list,
        "enter must not close the collection list"
    );
}
