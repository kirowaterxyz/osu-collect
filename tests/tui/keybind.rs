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
fn q_on_home_tab_shows_toast_first() {
    let mut app = make_app();
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(cmd.is_none(), "first q must not quit immediately");
    assert!(app.home.quit_prompt, "first q must set the quit toast");
}

#[test]
fn q_twice_on_home_tab_quits() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::Char('q')));
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(matches!(cmd, Some(AppCommand::Quit)));
}

#[test]
fn esc_on_home_tab_shows_toast_first() {
    let mut app = make_app();
    let cmd = app.handle_key(press(KeyCode::Esc));
    assert!(cmd.is_none(), "first esc must not quit immediately");
    assert!(app.home.quit_prompt, "first esc must set the quit toast");
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
    // should wrap to last field (the download button)
    assert_eq!(app.home.focus, HomeField::Download);
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

// ── enter toggle on mirrors ───────────────────────────────────────────────────

#[test]
fn enter_on_mirror_field_toggles_state() {
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
    app.handle_key(press(KeyCode::Enter));
    assert_eq!(app.home.nerinyan, !before);
}

#[test]
fn space_on_mirror_field_also_toggles_state() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    app.home.focus = HomeField::MirrorNerinyan;

    let before = app.home.nerinyan;
    app.handle_key(press(KeyCode::Char(' ')));
    assert_eq!(
        app.home.nerinyan, !before,
        "space toggles checkboxes as an alias for enter"
    );
}

#[test]
fn space_on_download_button_does_not_start_download() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    app.home.collection.value = "123".to_string();
    app.home.focus = HomeField::Download;

    // space is a toggle alias only; it must not activate the download button
    let cmd = app.handle_key(press(KeyCode::Char(' ')));
    assert!(
        cmd.is_none(),
        "space on the download button must not start a download"
    );
}

#[test]
fn space_inside_collection_list_toggles_focused_item() {
    let mut app = make_app();
    app.next_tab();
    // seed one collection and drop into the list
    app.updates
        .set_collections(vec![osu_collect::osu_db::LocalCollection {
            name: "test - 1234".to_string(),
            beatmap_checksums: Vec::new().into(),
        }]);
    app.updates.selection.in_collection_list = true;
    app.updates.selection.collections_state = Some(0);

    let before = app.updates.selection.local_collections[0].selected;
    app.handle_key(press(KeyCode::Char(' ')));
    assert_eq!(
        app.updates.selection.local_collections[0].selected, !before,
        "space toggles the focused list selection"
    );
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
fn enter_on_download_button_without_collection_input_produces_error() {
    use osu_collect::app::HomeField;
    let mut app = make_app();
    // clear any default value
    app.home.collection.value.clear();
    // focus the download button; enter there should fail to download
    app.home.focus = HomeField::Download;
    app.handle_key(press(KeyCode::Enter));
    // no command issued (error path), message should be set
    assert!(app.home.message.is_some());
}

#[test]
fn enter_on_collection_field_does_not_start_download() {
    let mut app = make_app();
    app.home.collection.value.clear();
    // collection field is focused by default; enter here only acts on the field,
    // it must not attempt a download (that lives on the button now)
    app.handle_key(press(KeyCode::Enter));
    assert!(
        app.home.message.is_none(),
        "enter on the collection field must not trigger the download error path"
    );
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
    app.config.focus = ConfigField::AuthChip;

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
fn space_on_auth_chip_does_nothing() {
    use osu_collect::app::ConfigField;
    use osu_collect::config::constants::CONFIG_TAB_INDEX;

    let mut app = make_app();
    app.handle_key(press(KeyCode::Right));
    app.handle_key(press(KeyCode::Right));
    assert_eq!(app.active_tab(), CONFIG_TAB_INDEX);
    app.config.focus = ConfigField::AuthChip;

    // space must not trigger any action on the chip — enter is the confirm key
    let cmd = app.handle_key(press(KeyCode::Char(' ')));
    assert!(
        cmd.is_none(),
        "space on auth chip must not issue any command"
    );
}

// ── help overlay ─────────────────────────────────────────────────────────────

#[test]
fn question_mark_opens_help_overlay() {
    let mut app = make_app();
    assert!(!app.help_open);
    app.handle_key(press(KeyCode::Char('?')));
    assert!(app.help_open, "? must open the help overlay");
}

#[test]
fn question_mark_closes_open_help_overlay() {
    let mut app = make_app();
    app.help_open = true;
    app.handle_key(press(KeyCode::Char('?')));
    assert!(!app.help_open, "? must close an already-open help overlay");
}

#[test]
fn esc_closes_help_overlay_without_quitting() {
    let mut app = make_app();
    app.help_open = true;
    let cmd = app.handle_key(press(KeyCode::Esc));
    assert!(!app.help_open, "esc must close the help overlay");
    assert!(cmd.is_none(), "esc while help is open must not quit");
}

#[test]
fn q_closes_help_overlay_without_quitting() {
    let mut app = make_app();
    app.help_open = true;
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(!app.help_open, "q must close the help overlay");
    assert!(cmd.is_none(), "q while help is open must not quit");
}

#[test]
fn question_mark_returns_no_command() {
    let mut app = make_app();
    let cmd = app.handle_key(press(KeyCode::Char('?')));
    assert!(cmd.is_none(), "? must not issue any AppCommand");
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
