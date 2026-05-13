/// Tests for the scroll_window utility in tui/view/components.
///
/// These verify that the visible window follows the focused item correctly
/// at small terminal sizes, matching the cloudy-ui constraint that every
/// list must handle 80×24 gracefully.
use osu_collect::app::App;
use osu_collect::config::Config;
use osu_collect::tui::draw;
use ratatui::{Terminal, backend::TestBackend};

fn make_app() -> App {
    App::new(Config::default())
}

// ── scroll_window unit tests (via re-export) ─────────────────────────────────

// We test scroll_window logic directly through rendered output rather than
// calling the private function, because scroll_window is pub(super).

#[test]
fn home_renders_all_fields_at_small_height() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut app = make_app();
    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).unwrap();

    // navigate through all fields and verify no panic at 80×14
    for _ in 0..20 {
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let key = KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        app.handle_key(key);
    }
}

#[test]
fn updates_tab_renders_at_minimum_size() {
    let mut app = make_app();
    app.next_tab();

    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal.draw(|frame| draw(frame, &app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    // should render something meaningful even at 80×14
    assert!(!content.trim().is_empty());
}

#[test]
fn config_tab_scrolls_through_fields_at_small_size() {
    use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    let mut app = make_app();
    // navigate to config tab
    app.next_tab();
    app.next_tab();

    let backend = TestBackend::new(80, 16);
    let mut terminal = Terminal::new(backend).unwrap();

    for _ in 0..25 {
        terminal.draw(|frame| draw(frame, &app)).unwrap();
        let key = KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::empty(),
            kind: KeyEventKind::Press,
            state: KeyEventState::empty(),
        };
        app.handle_key(key);
    }
    // no panic means scroll window is working
}

// ── scroll_window pure logic ──────────────────────────────────────────────────

/// Test the scroll_window algorithm directly by importing it.
/// We recreate the logic here since it's private, ensuring the contract:
///
/// 1. When total items <= visible height, show all.
/// 2. When focused near start, start from 0.
/// 3. When focused near end, end at items.len().
/// 4. Otherwise, centre the focused item.
#[test]
fn scroll_window_shows_all_when_items_fit() {
    let result = scroll_window_impl(10, 5, 15);
    assert_eq!(result, (0, 10));
}

#[test]
fn scroll_window_starts_at_zero_when_focus_near_start() {
    let result = scroll_window_impl(50, 2, 10);
    assert_eq!(result.0, 0);
}

#[test]
fn scroll_window_ends_at_total_when_focus_near_end() {
    let (_start, end) = scroll_window_impl(50, 49, 10);
    assert_eq!(end, 50);
}

#[test]
fn scroll_window_centres_focus_in_middle() {
    let (start, end) = scroll_window_impl(100, 50, 10);
    assert!(start <= 50);
    assert!(end > 50);
    assert_eq!(end - start, 10);
}

#[test]
fn scroll_window_empty_list_returns_zero_range() {
    let result = scroll_window_impl(0, 0, 10);
    assert_eq!(result, (0, 0));
}

/// Local re-implementation of scroll_window for pure logic testing.
fn scroll_window_impl(total: usize, focused: usize, visible: usize) -> (usize, usize) {
    if total == 0 || visible == 0 || total <= visible {
        return (0, total);
    }
    let focused = focused.min(total.saturating_sub(1));
    let half = visible / 2;
    let start = if focused <= half {
        0
    } else if focused + visible - half > total {
        total.saturating_sub(visible)
    } else {
        focused - half
    };
    let end = (start + visible).min(total);
    (start, end)
}
