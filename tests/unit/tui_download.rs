use super::{format_avg_verify, format_eta, session_eta};
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
//
// session_eta now reads speed from cumulative_speed() (the rolling average the
// OVERVIEW panel also uses) rather than recomputing bytes_done / elapsed.
// Tests that need a non-zero speed inject it via set_cached_speed_for_test.

use crate::app::collection::CollectionPage;

fn page_with_speed(speed: f64, bytes_done: u64, total_bytes: Option<u64>) -> CollectionPage {
    let mut page = CollectionPage::new(1, "test".to_string(), 1);
    page.stats.bytes_downloaded = bytes_done;
    page.stats.total_collection_bytes = total_bytes;
    page.set_cached_speed_for_test(speed);
    page
}

/// No ETA when speed is zero (no active downloads yet).
#[test]
fn session_eta_none_when_no_speed() {
    let mut page = CollectionPage::new(1, "test".to_string(), 1);
    page.stats.bytes_downloaded = 1024 * 1024;
    page.stats.total_collection_bytes = Some(10 * 1024 * 1024);
    // cached speed defaults to 0.0 — no active lines
    assert!(session_eta(&page).is_none());
}

/// No ETA when total collection size is unknown.
#[test]
fn session_eta_none_when_no_total() {
    let page = page_with_speed(5.0 * 1024.0 * 1024.0, 1024 * 1024, None);
    assert!(session_eta(&page).is_none());
}

/// When downloaded >= total, remaining is 0 and ETA should be "0s".
#[test]
fn session_eta_zero_remaining_when_done() {
    let total = 5 * 1024 * 1024u64;
    let page = page_with_speed(5.0 * 1024.0 * 1024.0, total + 1024, Some(total));
    let (_, eta) = session_eta(&page).expect("should compute ETA");
    assert_eq!(eta, "0s");
}

/// Happy path: reasonable speed and ETA strings are returned.
#[test]
fn session_eta_returns_speed_and_eta() {
    // 5 MB/s rolling speed; 450 MB remaining → 90 s ETA.
    let mb = 1024 * 1024u64;
    let speed = 5.0 * mb as f64;
    let page = page_with_speed(speed, 50 * mb, Some(500 * mb));
    let (speed_str, eta) = session_eta(&page).expect("should compute ETA");
    assert!(
        speed_str.contains("MB/s"),
        "expected MB/s in speed: {speed_str:?}"
    );
    assert!(
        eta.contains('m') || eta.contains('s'),
        "expected time unit in eta: {eta:?}"
    );
}
