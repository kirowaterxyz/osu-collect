use crate::app::url_history::{UrlHistoryEntry, UrlHistoryFile, push};

fn entry(url: &str) -> UrlHistoryEntry {
    UrlHistoryEntry {
        url: url.to_string(),
        name: format!("Collection for {url}"),
        count: 10,
        last_used: "2026-01-01T00:00:00Z".to_string(),
    }
}

// ── storage roundtrip ────────────────────────────────────────────────────────

#[test]
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("url-history.json");

    let mut history = UrlHistoryFile::default();
    push(
        &mut history,
        entry("https://osucollector.com/collections/1"),
    );
    push(
        &mut history,
        entry("https://osucollector.com/collections/2"),
    );

    // save directly via the public free-fn that takes a path
    let contents = serde_json::to_string_pretty(&history).unwrap();
    std::fs::write(&path, &contents).unwrap();

    let loaded: UrlHistoryFile =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    assert_eq!(loaded.entries.len(), 2);
    assert_eq!(
        loaded.entries[0].url,
        "https://osucollector.com/collections/2"
    );
    assert_eq!(
        loaded.entries[1].url,
        "https://osucollector.com/collections/1"
    );
}

// ── dedupe ───────────────────────────────────────────────────────────────────

#[test]
fn push_same_url_twice_does_not_grow_list() {
    let mut history = UrlHistoryFile::default();
    push(
        &mut history,
        entry("https://osucollector.com/collections/42"),
    );
    push(
        &mut history,
        entry("https://osucollector.com/collections/42"),
    );

    assert_eq!(history.entries.len(), 1);
}

#[test]
fn push_same_url_moves_it_to_front() {
    let mut history = UrlHistoryFile::default();
    push(
        &mut history,
        entry("https://osucollector.com/collections/1"),
    );
    push(
        &mut history,
        entry("https://osucollector.com/collections/2"),
    );
    push(
        &mut history,
        entry("https://osucollector.com/collections/1"),
    );

    assert_eq!(history.entries.len(), 2);
    assert_eq!(
        history.entries[0].url,
        "https://osucollector.com/collections/1"
    );
}

// ── cap at 10 ────────────────────────────────────────────────────────────────

#[test]
fn cap_at_10_drops_oldest() {
    let mut history = UrlHistoryFile::default();
    for i in 1..=11 {
        push(
            &mut history,
            entry(&format!("https://osucollector.com/collections/{i}")),
        );
    }

    assert_eq!(history.entries.len(), 10);
    // The oldest (1) was pushed first and is now gone; newest (11) is at index 0.
    assert_eq!(
        history.entries[0].url,
        "https://osucollector.com/collections/11"
    );
    assert!(!history.entries.iter().any(|e| e.url.ends_with("/1")));
}

// ── dropdown key behaviour (via HomeTab methods) ─────────────────────────────

#[test]
fn up_down_while_dropdown_open_does_not_change_focus() {
    use crate::app::{App, AppCommand};
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = App::new(Config::default());
    // Pre-populate history so the dropdown can open.
    push(
        &mut app.home.url_history,
        entry("https://osucollector.com/collections/1"),
    );
    push(
        &mut app.home.url_history,
        entry("https://osucollector.com/collections/2"),
    );
    app.home.dropdown_open = true;
    app.home.dropdown_selected = Some(0);

    let initial_focus = app.home.focus;

    let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
    let cmd = app.handle_key(down);

    // Focus must not change — dropdown intercepted the key.
    assert_eq!(
        app.home.focus, initial_focus,
        "focus must not change while dropdown is open"
    );
    // No field-navigation command returned.
    assert!(
        !matches!(cmd, Some(AppCommand::ResolveCollectionUrl { .. })),
        "down while dropdown open should not emit resolve command"
    );
    // Selection moved to next entry.
    assert_eq!(app.home.dropdown_selected, Some(1));
}

#[test]
fn enter_while_dropdown_open_fills_field_and_triggers_resolve() {
    use crate::app::{App, AppCommand};
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = App::new(Config::default());
    push(
        &mut app.home.url_history,
        entry("https://osucollector.com/collections/99"),
    );
    app.home.dropdown_open = true;
    app.home.dropdown_selected = Some(0);

    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    let cmd = app.handle_key(enter);

    assert!(!app.home.dropdown_open, "dropdown must close after enter");
    assert_eq!(
        app.home.collection.value,
        "https://osucollector.com/collections/99"
    );
    assert!(
        matches!(cmd, Some(AppCommand::ResolveCollectionUrl { ref value }) if value == "https://osucollector.com/collections/99"),
        "expected ResolveCollectionUrl, got {cmd:?}"
    );
}

#[test]
fn typing_char_closes_dropdown() {
    use crate::app::App;
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = App::new(Config::default());
    push(
        &mut app.home.url_history,
        entry("https://osucollector.com/collections/1"),
    );
    app.home.dropdown_open = true;
    app.home.dropdown_selected = Some(0);

    let key = KeyEvent::new(KeyCode::Char('h'), KeyModifiers::NONE);
    app.handle_key(key);

    assert!(
        !app.home.dropdown_open,
        "typing a char must close the dropdown"
    );
}

#[test]
fn esc_closes_dropdown_before_quit() {
    use crate::app::App;
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = App::new(Config::default());
    push(
        &mut app.home.url_history,
        entry("https://osucollector.com/collections/1"),
    );
    app.home.dropdown_open = true;
    app.home.dropdown_selected = Some(0);

    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let cmd = app.handle_key(esc);

    assert!(!app.home.dropdown_open, "esc must close the dropdown");
    assert!(cmd.is_none(), "esc while dropdown open must not quit");
}

#[test]
fn esc_with_help_and_dropdown_open_closes_help_first() {
    use crate::app::App;
    use crate::config::Config;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let mut app = App::new(Config::default());
    push(
        &mut app.home.url_history,
        entry("https://osucollector.com/collections/1"),
    );
    app.home.dropdown_open = true;
    app.home.dropdown_selected = Some(0);
    app.help_open = true;

    let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
    let cmd = app.handle_key(esc);

    assert!(!app.help_open, "esc must close help overlay first");
    assert!(app.home.dropdown_open, "dropdown must remain open");
    assert!(cmd.is_none(), "esc must not quit");
}
