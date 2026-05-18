use crate::{
    app::{App, AppCommand},
    config::{
        Config,
        constants::{CONFIG_TAB_INDEX, HOME_TAB_INDEX, UPDATES_TAB_INDEX},
    },
};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn right_tab_switch_ignores_stale_updates_list_on_home() {
    let mut app = App::new(Config::default());
    app.active_tab = HOME_TAB_INDEX;
    app.updates.selection.in_collection_list = true;

    let cmd = app.handle_key(key(KeyCode::Right));

    assert_eq!(app.active_tab, UPDATES_TAB_INDEX);
    assert!(matches!(cmd, Some(AppCommand::ScanLocalDatabase)));
}

#[test]
fn left_tab_switch_ignores_stale_updates_list_on_config() {
    let mut app = App::new(Config::default());
    app.active_tab = CONFIG_TAB_INDEX;
    app.updates.selection.in_beatmap_list = true;

    let cmd = app.handle_key(key(KeyCode::Left));

    assert_eq!(app.active_tab, UPDATES_TAB_INDEX);
    assert!(matches!(cmd, Some(AppCommand::ScanLocalDatabase)));
}

#[test]
fn tab_switch_stays_locked_inside_updates_list() {
    let mut app = App::new(Config::default());
    app.active_tab = UPDATES_TAB_INDEX;
    app.updates.selection.in_collection_list = true;

    let cmd = app.handle_key(key(KeyCode::Right));

    assert_eq!(app.active_tab, UPDATES_TAB_INDEX);
    assert!(cmd.is_none());
}
