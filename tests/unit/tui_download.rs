use super::{done_footer_spans, format_avg_verify, format_eta, session_eta};
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

// ── format_eta ───────────────────────────────────────────────────────────────

#[test]
fn format_eta_sub_minute() {
    assert_eq!(format_eta(0), "0s");
    assert_eq!(format_eta(1), "1s");
    assert_eq!(format_eta(45), "45s");
    assert_eq!(format_eta(59), "59s");
}

#[test]
fn format_eta_minutes() {
    assert_eq!(format_eta(60), "1m 00s");
    assert_eq!(format_eta(90), "1m 30s");
    assert_eq!(format_eta(150), "2m 30s");
    assert_eq!(format_eta(3599), "59m 59s");
}

#[test]
fn format_eta_hours() {
    assert_eq!(format_eta(3600), "1h 00m");
    assert_eq!(format_eta(3660), "1h 01m");
    assert_eq!(format_eta(4320), "1h 12m");
    assert_eq!(format_eta(7200), "2h 00m");
}

// ── session_eta ───────────────────────────────────────────────────────────────

use crate::app::collection::CollectionPage;
use std::time::{Duration, Instant};

fn page_for_eta(bytes_done: u64, total_bytes: Option<u64>, elapsed: Duration) -> CollectionPage {
    let mut page = CollectionPage::new(1, "test".to_string(), 1);
    page.stats.bytes_downloaded = bytes_done;
    page.stats.total_collection_bytes = total_bytes;
    // Back-date the session start so elapsed matches what we intend.
    page.session_start = Some(Instant::now() - elapsed);
    page
}

/// No ETA when session start is not set.
#[test]
fn session_eta_none_when_no_start() {
    let mut page = CollectionPage::new(1, "test".to_string(), 1);
    page.stats.bytes_downloaded = 1024 * 1024;
    page.stats.total_collection_bytes = Some(10 * 1024 * 1024);
    assert!(session_eta(&page).is_none());
}

/// No ETA when elapsed < 1 s (too early to trust the average).
#[test]
fn session_eta_none_when_elapsed_under_1s() {
    let page = page_for_eta(1024, Some(10 * 1024), Duration::from_millis(200));
    assert!(session_eta(&page).is_none());
}

/// No ETA when no bytes have been downloaded yet.
#[test]
fn session_eta_none_when_zero_bytes() {
    let page = page_for_eta(0, Some(10 * 1024 * 1024), Duration::from_secs(5));
    assert!(session_eta(&page).is_none());
}

/// No ETA when total collection size is unknown.
#[test]
fn session_eta_none_when_no_total() {
    let page = page_for_eta(1024 * 1024, None, Duration::from_secs(5));
    assert!(session_eta(&page).is_none());
}

/// When downloaded >= total, remaining is 0 and ETA should be "0s".
#[test]
fn session_eta_zero_remaining_when_done() {
    let total = 5 * 1024 * 1024u64;
    let page = page_for_eta(total + 1024, Some(total), Duration::from_secs(10));
    let (_, eta) = session_eta(&page).expect("should compute ETA");
    assert_eq!(eta, "0s");
}

/// Happy path: reasonable speed and ETA strings are returned.
#[test]
fn session_eta_returns_speed_and_eta() {
    // 50 MB downloaded in 10 s → 5 MB/s; 450 MB remaining → 90 s ETA.
    // Use 50× to stay well above the KB/MB boundary regardless of timing jitter.
    let mb = 1024 * 1024u64;
    let page = page_for_eta(50 * mb, Some(500 * mb), Duration::from_secs(10));
    let (speed, eta) = session_eta(&page).expect("should compute ETA");
    assert!(speed.contains("MB/s"), "expected MB/s in speed: {speed:?}");
    assert!(
        eta.contains('m') || eta.contains('s'),
        "expected time unit in eta: {eta:?}"
    );
}
