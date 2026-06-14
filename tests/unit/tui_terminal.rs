use super::{TerminalGuard, reset_terminal_bg, set_terminal_bg};
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

// The guard's `Drop` writes real escapes to stdout and calls `ratatui::restore`
// (disables raw mode) — running it under the test harness would mutate the
// runner's terminal and is meaningless without a tty, so we don't invoke it.
// Instead pin its shape: a ZST whose only job is the teardown obligation, so
// holding one in `run` is free and the teardown lives in exactly one Drop.
#[test]
fn terminal_guard_is_a_zero_sized_teardown_obligation() {
    assert_eq!(std::mem::size_of::<TerminalGuard>(), 0);
    // A bare ZST drops trivially; `needs_drop` is true only because of our
    // `Drop` impl, so this pins that the teardown obligation is still wired.
    assert!(std::mem::needs_drop::<TerminalGuard>());
}
