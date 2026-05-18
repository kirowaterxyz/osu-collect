use super::super::{SPINNER_FRAMES, spinner_char};
use super::hint_line;

#[test]
fn spinner_wraps_correctly() {
    for tick in 0u64..30 {
        let frame = spinner_char(tick);
        assert!(SPINNER_FRAMES.contains(&frame));
    }
}

#[test]
fn hint_line_has_key_and_label_spans() {
    let line = hint_line("↑↓ move  ·  q quit");
    let full: String = line.spans.iter().map(|s| s.content.as_ref()).collect();
    assert!(full.contains("↑↓"));
    assert!(full.contains("move"));
    assert!(full.contains("q"));
    assert!(full.contains("quit"));
}
