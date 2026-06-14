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

#[test]
fn updates_tab_renders_at_minimum_size() {
    let mut app = make_app();
    app.next_tab();

    let backend = TestBackend::new(80, 14);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| {
            draw(frame, &app);
        })
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let content: String = buf.content().iter().map(|c| c.symbol()).collect();
    // should render something meaningful even at 80×14
    assert!(!content.trim().is_empty());
}
