/// Quit confirmation toast behaviour.
///
/// First `q`/`esc` shows a toast and does NOT quit.
/// Second `q`/`esc` while the toast is visible quits.
/// Any other key while the toast is visible dismisses it and falls through.
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

// ── first q shows toast, does not quit ───────────────────────────────────────

#[test]
fn first_q_sets_quit_prompt_no_downloads() {
    let mut app = make_app();
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(cmd.is_none(), "first q must not quit");
    assert!(app.home.quit_prompt, "first q must raise the quit toast");
}

#[test]
fn first_esc_sets_quit_prompt_no_downloads() {
    let mut app = make_app();
    let cmd = app.handle_key(press(KeyCode::Esc));
    assert!(cmd.is_none(), "first esc must not quit");
    assert!(app.home.quit_prompt, "first esc must raise the quit toast");
}

// ── second q while toast visible quits ───────────────────────────────────────

#[test]
fn second_q_quits() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::Char('q')));
    assert!(app.home.quit_prompt);
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(matches!(cmd, Some(AppCommand::Quit)));
    assert!(!app.home.quit_prompt, "quit_prompt must be cleared on quit");
}

#[test]
fn second_esc_quits() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::Esc));
    assert!(app.home.quit_prompt);
    let cmd = app.handle_key(press(KeyCode::Esc));
    assert!(matches!(cmd, Some(AppCommand::Quit)));
    assert!(!app.home.quit_prompt, "quit_prompt must be cleared on quit");
}

// ── unrelated key clears toast and falls through ──────────────────────────────

#[test]
fn tab_clears_toast_and_switches_tab() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::Char('q')));
    assert!(app.home.quit_prompt);
    let tab_before = app.active_tab();
    app.handle_key(press(KeyCode::Tab));
    assert!(!app.home.quit_prompt, "tab must clear the quit toast");
    assert_ne!(
        app.active_tab(),
        tab_before,
        "tab must still switch the active tab"
    );
}

#[test]
fn any_char_key_clears_toast_without_quitting() {
    let mut app = make_app();
    app.handle_key(press(KeyCode::Char('q')));
    assert!(app.home.quit_prompt);
    // pressing a letter key (not q) should clear the toast
    let cmd = app.handle_key(press(KeyCode::Char('a')));
    assert!(!app.home.quit_prompt, "non-quit key must clear the toast");
    assert!(
        cmd.is_none(),
        "non-quit key after toast must not issue a quit command"
    );
}

// ── modal takes priority over quit toast ─────────────────────────────────────

#[test]
fn q_with_help_open_closes_modal_not_toast() {
    let mut app = make_app();
    app.help_open = true;
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(!app.help_open, "q must close the help modal");
    assert!(
        !app.home.quit_prompt,
        "quit toast must not be raised when a modal was closed"
    );
    assert!(cmd.is_none());
}

#[test]
fn q_after_modal_closed_then_shows_toast() {
    let mut app = make_app();
    // open and close modal, then press q again
    app.help_open = true;
    app.handle_key(press(KeyCode::Char('q'))); // closes modal
    assert!(!app.help_open);
    assert!(!app.home.quit_prompt);
    // now q with no modal → toast
    let cmd = app.handle_key(press(KeyCode::Char('q')));
    assert!(
        cmd.is_none(),
        "q after modal closes must show toast, not quit"
    );
    assert!(
        app.home.quit_prompt,
        "toast must be raised after modal is gone"
    );
}
