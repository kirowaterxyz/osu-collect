use super::{format_avg_verify, format_speed, summarize_failure};
use crate::config::constants::MAX_TRUNCATED_CHARS;

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
fn format_speed_units() {
    assert_eq!(format_speed(500.0), "500 B/s");
    assert_eq!(format_speed(1024.0), "1.0 KB/s");
    assert_eq!(format_speed(1024.0 * 1024.0), "1.00 MB/s");
}

#[test]
fn summarize_failure_truncates_long() {
    let reason = "x".repeat(200);
    let summary = summarize_failure(&reason);
    assert!(summary.ends_with("..."));
    assert!(summary.chars().count() <= MAX_TRUNCATED_CHARS);
}

#[test]
fn summarize_failure_empty_returns_unknown() {
    assert_eq!(summarize_failure(""), "unknown error");
}
