use super::{done_footer_spans, format_avg_verify};
use crate::utils::format_bytes;

#[test]
fn format_avg_verify_us_boundary() {
    assert_eq!(format_avg_verify(0), "0us");
    assert_eq!(format_avg_verify(999), "999us");
    assert_eq!(format_avg_verify(1_000), "1.0ms");
    assert_eq!(format_avg_verify(999_999), "1000.0ms");
    assert_eq!(format_avg_verify(1_000_000), "1.0s");
    assert_eq!(format_avg_verify(59_999_999), "60.0s");
    assert_eq!(format_avg_verify(60_000_000), "1.0m");
    assert_eq!(format_avg_verify(120_000_000), "2.0m");
}

#[test]
fn format_bytes_units() {
    assert_eq!(format_bytes(500, "B/s"), "500 B/s");
    assert_eq!(format_bytes(1024, "B/s"), "1 KB/s");
    assert_eq!(format_bytes(1024 * 1024, "B/s"), "1.0 MB/s");
    assert_eq!(format_bytes(2 * 1024 * 1024 * 1024, "B"), "2.00 GB");
    assert_eq!(format_bytes(999, "B"), "999 B");
}

/// Collect the plain text of all spans in the footer, ignoring glyph choices.
fn footer_text(done: u32, skipped: u32) -> String {
    done_footer_spans(done, skipped)
        .iter()
        .map(|s| s.content.as_ref())
        .collect()
}

/// The footer spans contain the "done" label and the count when done > 0.
#[test]
fn done_footer_appears_when_downloaded_nonzero() {
    let text = footer_text(5, 0);
    assert!(text.contains("done"), "expected 'done' in footer: {text:?}");
    assert!(text.contains('5'), "expected count '5' in footer: {text:?}");
}

/// `done_footer_spans` always emits the "done" label regardless of the count.
/// The call-site guard (`if done > 0`) is what suppresses rendering at zero.
#[test]
fn done_footer_label_stable_at_zero() {
    // The function itself has no internal guard; it always emits the label.
    let text = footer_text(0, 0);
    assert!(
        text.contains("done"),
        "done_footer_spans must always emit the 'done' label: {text:?}"
    );
}

/// The skipped segment is included only when skipped > 0.
#[test]
fn done_footer_includes_skipped_when_nonzero() {
    let text = footer_text(3, 2);
    assert!(
        text.contains("skipped"),
        "expected 'skipped' in footer: {text:?}"
    );
    assert!(
        text.contains('2'),
        "expected skipped count '2' in footer: {text:?}"
    );
}

/// No skipped segment when skipped == 0.
#[test]
fn done_footer_omits_skipped_when_zero() {
    let text = footer_text(3, 0);
    assert!(
        !text.contains("skipped"),
        "footer must not contain 'skipped': {text:?}"
    );
}
