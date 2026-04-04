use osu_collect::download::error::DownloadError;

#[test]
fn retryable_errors() {
    assert!(DownloadError::RateLimited.is_retryable());
    assert!(DownloadError::timeout("stalled").is_retryable());
}

#[test]
fn non_retryable_errors() {
    assert!(!DownloadError::NoMirrors.is_retryable());
    assert!(!DownloadError::EmptyCollection.is_retryable());
    assert!(!DownloadError::NoBeatmapsets.is_retryable());
    assert!(!DownloadError::Aborted.is_retryable());
    assert!(!DownloadError::invalid_archive("bad zip").is_retryable());
    assert!(!DownloadError::internal("oops").is_retryable());
}

#[test]
fn validation_error_variant() {
    let err = DownloadError::ValidationFailed {
        beatmapset_id: 12345,
        reason: "bad hash".into(),
    };
    assert!(!err.is_retryable());
    let display = err.to_string();
    assert!(display.contains("12345"));
    assert!(display.contains("bad hash"));
}

#[test]
fn worker_panic_variant() {
    let err = DownloadError::worker_panic("thread panicked");
    assert!(!err.is_retryable());
}
