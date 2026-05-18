use super::{
    BeatmapsetDownloadCallbacks, BeatmapsetDownloadOptions, BeatmapsetDownloadOutcome,
    DownloadParams, FinalizeResult, download_beatmapset, finalize_download,
    is_archive_content_type, matches_beatmapset, probe_download_size, sanitize_filename,
    size_from_content_range, sleep_cancelable,
};
use crate::mirrors::pool::MirrorPool;
use crate::validation::minimal_zip_bytes_for_test;
use crate::{
    ArchiveValidation, FileExistsPolicy, Mirror, MirrorKind, SkipReason, StatusEvent,
};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

fn default_params<'a>(
    beatmapset_id: u32,
    output_dir: &'a Path,
    client: &'a reqwest::Client,
    mirror_pool: &'a MirrorPool,
    cancel_rx: tokio::sync::watch::Receiver<bool>,
) -> DownloadParams<'a> {
    DownloadParams {
        beatmapset_id,
        output_dir,
        client,
        mirror_pool,
        archive_validation: ArchiveValidation::Off,
        progress_timeout: Duration::from_secs(1),
        callbacks: BeatmapsetDownloadCallbacks::default(),
        options: BeatmapsetDownloadOptions::default(),
        cancel_rx,
    }
}

#[test]
fn test_sanitize_filename() {
    assert_eq!(sanitize_filename(None, 123), "123.osz");
    assert_eq!(
        sanitize_filename(Some("test/file.osz"), 456),
        "test_file.osz"
    );
    assert_eq!(sanitize_filename(Some(".."), 789), "789.osz");
    assert_eq!(sanitize_filename(Some("."), 789), "789.osz");
    assert_eq!(sanitize_filename(Some(""), 789), "789.osz");
    assert_eq!(sanitize_filename(Some("./map.osz"), 789), "789.osz");
    assert_eq!(sanitize_filename(Some("../etc/passwd"), 789), "789.osz");
    assert_eq!(
        sanitize_filename(Some("normal map.osz"), 789),
        "normal map.osz"
    );
    assert_eq!(
        sanitize_filename(Some("ユニコード.osz"), 789),
        "ユニコード.osz"
    );
}

#[test]
fn test_extract_filename() {
    use super::extract_filename_from_header;
    assert_eq!(
        extract_filename_from_header("attachment; filename=\"test.osz\""),
        Some("test.osz".to_string())
    );

    assert_eq!(
        extract_filename_from_header("attachment; filename*=UTF-8''test%20file.osz"),
        Some("test file.osz".to_string())
    );

    assert_eq!(
        extract_filename_from_header(r#"attachment; filename="foo\"bar.osz""#),
        Some(r#"foo"bar.osz"#.to_string())
    );

    assert_eq!(
        extract_filename_from_header(r#"attachment; filename="foo\\bar.osz""#),
        Some(r#"foo\bar.osz"#.to_string())
    );

    assert_eq!(
        extract_filename_from_header("attachment; filename=plain.osz"),
        Some("plain.osz".to_string())
    );

    assert_eq!(
        extract_filename_from_header(r#"attachment; filename="artist - title; diff.osz""#),
        Some("artist - title; diff.osz".to_string())
    );

    assert_eq!(
        extract_filename_from_header(
            "attachment; filename=plain.osz; filename*=utf-8''encoded%20name.osz"
        ),
        Some("encoded name.osz".to_string())
    );

    assert_eq!(
        extract_filename_from_header(
            "attachment; filename=fallback.osz; filename*=iso-8859-1''ignored.osz"
        ),
        Some("fallback.osz".to_string())
    );

    assert_eq!(
        extract_filename_from_header("attachment; FILENAME=upper.osz"),
        Some("upper.osz".to_string())
    );
}

#[test]
fn matches_exact_beatmapset_file_names() {
    assert!(matches_beatmapset(123, "123.osz"));
    assert!(matches_beatmapset(123, "123 artist.osz"));
    assert!(!matches_beatmapset(123, "1234.osz"));
    assert!(!matches_beatmapset(123, "123artist.osz"));
    assert!(!matches_beatmapset(123, "123 artist.zip"));
}

#[test]
fn archive_content_type_accepts_known_archive_mimes() {
    assert!(is_archive_content_type("application/x-osu-beatmap-archive"));
    assert!(is_archive_content_type(
        "application/octet-stream; charset=binary"
    ));
    assert!(is_archive_content_type("binary/octet-stream"));
    assert!(is_archive_content_type("application/zip"));
    assert!(is_archive_content_type("application/x-zip-compressed"));
    assert!(!is_archive_content_type("text/html"));
    assert!(!is_archive_content_type("application/json"));
}

#[test]
fn size_from_content_range_uses_complete_length() {
    assert_eq!(
        size_from_content_range("bytes 0-0/24413678"),
        Some(24_413_678)
    );
    assert_eq!(size_from_content_range("bytes 0-3/*"), None);
    assert_eq!(size_from_content_range("invalid"), None);
}

#[tokio::test]
async fn range_probe_discovers_redirected_download_size() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            if request.starts_with("GET /mirror/") {
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://{addr}/archive/42\r\nContent-Length: 0\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            } else if request.starts_with("GET /archive/") {
                stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/10000000\r\nContent-Length: 1\r\n\r\nP").unwrap();
            }
        }
    });

    let client = reqwest::Client::new();
    let mirror = Mirror::custom(format!("http://{addr}/mirror/{{id}}")).unwrap();
    let mirror_pool = MirrorPool::new(vec![mirror.clone()]);
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let dir = tempfile::tempdir().unwrap();
    let params = default_params(42, dir.path(), &client, &mirror_pool, cancel_rx);

    assert_eq!(
        probe_download_size(&mirror, &params).await,
        Some(10_000_000)
    );
    server.join().unwrap();
}

#[tokio::test]
async fn probe_preserves_range_across_multiple_redirects() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            if request.starts_with("GET /api/") {
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://{addr}/dl/997762\r\nContent-Length: 0\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            } else if request.starts_with("GET /dl/") {
                stream
                    .write_all(
                        format!(
                            "HTTP/1.1 302 Found\r\nLocation: http://{addr}/s3/997762.osz\r\nContent-Length: 0\r\n\r\n"
                        )
                        .as_bytes(),
                    )
                    .unwrap();
            } else if request.starts_with("GET /s3/") {
                assert!(request.contains("Range: bytes=0-0"));
                stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/44911016\r\nContent-Length: 1\r\n\r\nP").unwrap();
            }
        }
    });

    let client = reqwest::Client::new();
    let mirror = Mirror::custom(format!("http://{addr}/api/{{id}}")).unwrap();
    let mirror_pool = MirrorPool::new(vec![mirror.clone()]);
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let dir = tempfile::tempdir().unwrap();
    let params = default_params(997762, dir.path(), &client, &mirror_pool, cancel_rx);

    assert_eq!(
        probe_download_size(&mirror, &params).await,
        Some(44_911_016)
    );
    server.join().unwrap();
}

#[tokio::test]
async fn completion_uses_probed_size_when_download_is_chunked() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 2048];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            if request.contains("Range: bytes=0-0") {
                stream.write_all(b"HTTP/1.1 206 Partial Content\r\nContent-Range: bytes 0-0/262144\r\nContent-Length: 1\r\n\r\nP").unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=42.osz\r\nTransfer-Encoding: chunked\r\n\r\n40000\r\n").unwrap();
                stream.write_all(&vec![b'a'; 262_144]).unwrap();
                let _ = stream.write_all(b"\r\n0\r\n\r\n");
            }
        }
    });

    let client = reqwest::Client::new();
    let mirror = Mirror::custom(format!("http://{addr}/download/{{id}}")).unwrap();
    let mirror_pool = MirrorPool::new(vec![mirror]);
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_events = progress.clone();
    let dir = tempfile::tempdir().unwrap();

    let (outcome, _) = download_beatmapset(DownloadParams {
        callbacks: BeatmapsetDownloadCallbacks {
            progress: Some(Arc::new(move |downloaded, total| {
                progress_events.lock().unwrap().push((downloaded, total));
            })),
            status: None,
        },
        ..default_params(42, dir.path(), &client, &mirror_pool, cancel_rx)
    })
    .await;

    assert!(matches!(
        outcome,
        BeatmapsetDownloadOutcome::Success {
            size_bytes: 262_144,
            ..
        }
    ));
    server.join().unwrap();
}

#[tokio::test]
async fn skip_existing_file_does_not_emit_downloading() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).unwrap();
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=custom.osz\r\nContent-Length: 0\r\n\r\n",
            )
            .unwrap();
    });

    let client = reqwest::Client::new();
    let mirror = Mirror::custom(format!("http://{addr}/download/{{id}}")).unwrap();
    let mirror_pool = MirrorPool::new(vec![mirror]);
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let statuses = Arc::new(Mutex::new(Vec::new()));
    let status_events = statuses.clone();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("custom.osz"), b"existing").unwrap();

    let (outcome, _) = download_beatmapset(DownloadParams {
        callbacks: BeatmapsetDownloadCallbacks {
            progress: None,
            status: Some(Arc::new(move |status| {
                status_events.lock().unwrap().push(status);
            })),
        },
        options: BeatmapsetDownloadOptions {
            file_exists_policy: FileExistsPolicy::Skip,
        },
        ..default_params(42, dir.path(), &client, &mirror_pool, cancel_rx)
    })
    .await;

    assert!(matches!(
        outcome,
        BeatmapsetDownloadOutcome::Skipped {
            reason: SkipReason::AlreadyExists
        }
    ));
    assert!(
        !statuses
            .lock()
            .unwrap()
            .iter()
            .any(|status| matches!(status, StatusEvent::Downloading { .. }))
    );
    server.join().unwrap();
}

#[tokio::test]
async fn finalize_download_preserves_existing_output() {
    let dir = std::env::temp_dir().join(format!(
        "osu-downloader-finalize-{}-{:?}",
        std::process::id(),
        std::time::SystemTime::now()
    ));
    tokio::fs::create_dir(&dir).await.unwrap();

    let temp_path = dir.join("123.osz.tmp");
    let output_path = dir.join("123.osz");
    tokio::fs::write(&temp_path, b"new").await.unwrap();
    tokio::fs::write(&output_path, b"old").await.unwrap();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let finalized = finalize_download(&temp_path, &output_path, &cancel_rx).await;

    assert!(matches!(finalized, FinalizeResult::AlreadyExists));
    assert_eq!(tokio::fs::read(&output_path).await.unwrap(), b"old");
    assert!(!tokio::fs::try_exists(&temp_path).await.unwrap());

    tokio::fs::remove_dir_all(&dir).await.unwrap();
}

#[tokio::test]
async fn rate_limit_status_suppressed_when_other_mirror_succeeds() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for _ in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            if request.starts_with("GET /rate/") {
                stream
                    .write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            } else {
                stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=321.osz\r\nContent-Length: 4\r\n\r\ndata").unwrap();
            }
        }
    });

    let rate_limited =
        Mirror::with_kind_and_template(MirrorKind::Nerinyan, format!("http://{addr}/rate/{{id}}"));
    let healthy = Mirror::with_kind_and_template(
        MirrorKind::OsuDirect,
        format!("http://{addr}/ok/{{id}}"),
    );
    let mirror_pool = MirrorPool::new(vec![rate_limited, healthy]);
    let dir = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let statuses: Arc<Mutex<Vec<StatusEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = statuses.clone();
    let callbacks = BeatmapsetDownloadCallbacks {
        progress: None,
        status: Some(Arc::new(move |event| {
            recorder.lock().unwrap().push(event);
        })),
    };

    let (outcome, _) = download_beatmapset(DownloadParams {
        callbacks,
        ..default_params(321, dir.path(), &client, &mirror_pool, cancel_rx)
    })
    .await;

    assert!(matches!(outcome, BeatmapsetDownloadOutcome::Success { .. }));
    let recorded = statuses.lock().unwrap();
    assert!(
        !recorded
            .iter()
            .any(|event| matches!(event, StatusEvent::RateLimited { .. })),
        "rate-limit status must not flash when a sibling mirror succeeds: {recorded:?}"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn rate_limit_status_emitted_once_when_all_mirrors_throttled() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let rate_a_hits = Arc::new(AtomicUsize::new(0));
    let rate_b_hits = Arc::new(AtomicUsize::new(0));
    let server_a = rate_a_hits.clone();
    let server_b = rate_b_hits.clone();
    let server = thread::spawn(move || {
        loop {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            if request.starts_with("GET /a/") {
                let hit = server_a.fetch_add(1, Ordering::SeqCst);
                if hit >= 1 {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=555.osz\r\nContent-Length: 4\r\n\r\ndata").unwrap();
                    break;
                }
                stream
                    .write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            } else if request.starts_with("GET /b/") {
                server_b.fetch_add(1, Ordering::SeqCst);
                stream
                    .write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            }
        }
    });

    let mirror_a =
        Mirror::with_kind_and_template(MirrorKind::Nerinyan, format!("http://{addr}/a/{{id}}"));
    let mirror_b = Mirror::with_kind_and_template(
        MirrorKind::OsuDirect,
        format!("http://{addr}/b/{{id}}"),
    );
    let mirror_pool = MirrorPool::new(vec![mirror_a, mirror_b]);
    let dir = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let statuses: Arc<Mutex<Vec<StatusEvent>>> = Arc::new(Mutex::new(Vec::new()));
    let recorder = statuses.clone();
    let callbacks = BeatmapsetDownloadCallbacks {
        progress: None,
        status: Some(Arc::new(move |event| {
            recorder.lock().unwrap().push(event);
        })),
    };

    let (outcome, _) = download_beatmapset(DownloadParams {
        callbacks,
        ..default_params(555, dir.path(), &client, &mirror_pool, cancel_rx)
    })
    .await;

    assert!(matches!(outcome, BeatmapsetDownloadOutcome::Success { .. }));
    let recorded = statuses.lock().unwrap();
    let rate_limit_events: Vec<_> = recorded
        .iter()
        .filter(|event| matches!(event, StatusEvent::RateLimited { .. }))
        .collect();
    assert_eq!(
        rate_limit_events.len(),
        1,
        "exactly one rate-limit event expected once every mirror is throttled: {recorded:?}"
    );
    server.join().unwrap();
}

#[tokio::test]
async fn rate_limited_mirror_is_retried_after_other_mirrors_fail() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::atomic::{AtomicUsize, Ordering},
        thread,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let rate_hits = Arc::new(AtomicUsize::new(0));
    let missing_hits = Arc::new(AtomicUsize::new(0));
    let server_rate_hits = rate_hits.clone();
    let server_missing_hits = missing_hits.clone();
    let server = thread::spawn(move || {
        for _ in 0..3 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0u8; 1024];
            let n = stream.read(&mut request).unwrap();
            let request = String::from_utf8_lossy(&request[..n]);
            if request.starts_with("GET /rate/") {
                let hit = server_rate_hits.fetch_add(1, Ordering::SeqCst);
                if hit == 0 {
                    stream
                        .write_all(b"HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n")
                        .unwrap();
                } else {
                    stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=123.osz\r\nContent-Length: 4\r\n\r\ndata").unwrap();
                }
            } else if request.starts_with("GET /missing/") {
                server_missing_hits.fetch_add(1, Ordering::SeqCst);
                stream
                    .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            } else {
                stream
                    .write_all(b"HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n")
                    .unwrap();
            }
        }
    });

    let rate_limited_then_ok =
        Mirror::with_kind_and_template(MirrorKind::Nerinyan, format!("http://{addr}/rate/{{id}}"));
    let missing = Mirror::with_kind_and_template(
        MirrorKind::OsuDirect,
        format!("http://{addr}/missing/{{id}}"),
    );
    let mirror_pool = MirrorPool::new(vec![rate_limited_then_ok, missing]);
    let dir = tempfile::tempdir().unwrap();
    let client = reqwest::Client::new();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let (outcome, _) = download_beatmapset(default_params(
        123,
        dir.path(),
        &client,
        &mirror_pool,
        cancel_rx,
    ))
    .await;

    assert!(matches!(outcome, BeatmapsetDownloadOutcome::Success { .. }));
    assert_eq!(rate_hits.load(Ordering::SeqCst), 2);
    assert_eq!(missing_hits.load(Ordering::SeqCst), 1);
    server.join().unwrap();
}

#[tokio::test]
async fn verify_archive_records_nonzero_duration_when_enabled() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    let zip_bytes = minimal_zip_bytes_for_test();
    let len = zip_bytes.len();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut request = [0u8; 1024];
        let _ = stream.read(&mut request).unwrap();
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Disposition: attachment; filename=99.osz\r\nContent-Length: {len}\r\n\r\n"
        );
        stream.write_all(header.as_bytes()).unwrap();
        stream.write_all(&zip_bytes).unwrap();
    });

    let client = reqwest::Client::new();
    let mirror = Mirror::custom(format!("http://{addr}/dl/{{id}}")).unwrap();
    let mirror_pool = MirrorPool::new(vec![mirror]);
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);
    let dir = tempfile::tempdir().unwrap();

    let (outcome, _) = download_beatmapset(DownloadParams {
        archive_validation: ArchiveValidation::Eocd,
        ..default_params(99, dir.path(), &client, &mirror_pool, cancel_rx)
    })
    .await;

    match outcome {
        BeatmapsetDownloadOutcome::Success {
            verify_duration_us, ..
        } => assert!(
            verify_duration_us > 0,
            "verify_duration_us must be non-zero when verification runs (got {verify_duration_us}us)"
        ),
        other => panic!("expected Success outcome, got {other:?}"),
    }
    server.join().unwrap();
}

#[tokio::test]
async fn backoff_cancelled_before_expiry() {
    let (cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        let _ = cancel_tx.send(true);
    });

    let start = Instant::now();
    assert!(sleep_cancelable(Duration::from_secs(1), &cancel_rx).await);

    assert!(
        start.elapsed() < Duration::from_millis(200),
        "backoff should have been cut short by cancel signal"
    );
}
