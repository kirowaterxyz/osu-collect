use super::{format_avg_verify, summarize_failure};
use crate::config::constants::MAX_TRUNCATED_CHARS;
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
