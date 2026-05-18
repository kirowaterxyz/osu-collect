use super::{reset_terminal_bg, set_terminal_bg};
use ratatui::style::Color;

#[test]
fn set_terminal_bg_emits_osc11_with_hex_rgb() {
    let mut buf = Vec::new();
    set_terminal_bg(&mut buf, Color::Rgb(30, 30, 46)).unwrap();
    assert_eq!(buf, b"\x1b]11;rgb:1e/1e/2e\x1b\\");
}

#[test]
fn set_terminal_bg_skips_non_rgb_colors() {
    let mut buf = Vec::new();
    set_terminal_bg(&mut buf, Color::Reset).unwrap();
    assert!(buf.is_empty());
}

#[test]
fn reset_terminal_bg_emits_osc111() {
    let mut buf = Vec::new();
    reset_terminal_bg(&mut buf).unwrap();
    assert_eq!(buf, b"\x1b]111\x1b\\");
}
