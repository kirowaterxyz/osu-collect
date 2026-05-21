use ratatui::{Terminal, backend::TestBackend, layout::Rect, style::Modifier};

use super::render;

fn header_buffer_with_active(active: usize) -> ratatui::buffer::Buffer {
    let tabs: Vec<std::borrow::Cow<'static, str>> = ["home", "updates", "config"]
        .map(std::borrow::Cow::Borrowed)
        .into();
    let backend = TestBackend::new(80, 1);
    let mut terminal = Terminal::new(backend).expect("test backend should initialize");
    terminal
        .draw(|frame| {
            render(frame, Rect::new(0, 0, 80, 1), &tabs, active, None);
        })
        .expect("header should render");
    terminal.backend().buffer().clone()
}

#[test]
fn active_tab_has_underlined_modifier() {
    // active=0 → "home"; check that at least one cell of "home" carries UNDERLINED
    let buf = header_buffer_with_active(0);
    let has_underlined = buf.content.iter().any(|cell| {
        cell.symbol() == "h" && cell.style().add_modifier.contains(Modifier::UNDERLINED)
    });
    assert!(
        has_underlined,
        "active tab 'home' must carry UNDERLINED modifier on at least one cell"
    );
}

#[test]
fn inactive_tabs_do_not_have_underlined_modifier() {
    // active=0 → "home"; "updates" and "config" are inactive
    let buf = header_buffer_with_active(0);

    // Sample the first letter of each inactive tab title.
    // "updates" starts with 'u', "config" starts with 'c'.
    // Neither of these letters appears in "home", the brand, or the version on a 80-col render,
    // so checking the modifier on 'u' and 'c' cells is sufficient.
    let inactive_letters = ['u', 'c'];
    for letter in inactive_letters {
        let underlined_inactive = buf.content.iter().any(|cell| {
            cell.symbol() == letter.encode_utf8(&mut [0u8; 4])
                && cell.style().add_modifier.contains(Modifier::UNDERLINED)
        });
        assert!(
            !underlined_inactive,
            "inactive tab with first letter '{letter}' must not carry UNDERLINED modifier"
        );
    }
}
