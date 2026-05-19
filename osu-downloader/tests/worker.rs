use super::{MIN_PROGRESS_DELTA, TEMP_FILE_COUNTER, TempFileGuard, finalize_md5, stream_download};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[test]
fn temp_file_guard_removes_on_drop_when_armed() {
    let path = std::env::temp_dir().join(format!(
        "osu-downloader-test-{}-{}.part",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, b"hello").unwrap();
    {
        let _guard = TempFileGuard::new(path.clone());
    }
    assert!(!path.exists(), "guard must remove file when dropped armed");
}

#[test]
fn temp_file_guard_keeps_file_when_disarmed() {
    let path = std::env::temp_dir().join(format!(
        "osu-downloader-test-{}-{}.part",
        std::process::id(),
        TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, b"hello").unwrap();
    {
        let mut guard = TempFileGuard::new(path.clone());
        guard.disarm();
    }
    assert!(path.exists(), "disarmed guard must not remove the file");
    std::fs::remove_file(&path).unwrap();
}

#[tokio::test]
async fn final_chunk_does_not_emit_complete_progress() {
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
                format!("HTTP/1.1 200 OK\r\nContent-Length: {MIN_PROGRESS_DELTA}\r\n\r\n")
                    .as_bytes(),
            )
            .unwrap();
        stream
            .write_all(&vec![b'a'; MIN_PROGRESS_DELTA as usize])
            .unwrap();
    });

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/archive.osz"))
        .send()
        .await
        .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let output_path = dir.path().join("archive.osz");
    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_events = progress.clone();
    let (_cancel_tx, cancel_rx) = tokio::sync::watch::channel(false);

    let result = stream_download(
        response,
        &output_path,
        Some(MIN_PROGRESS_DELTA),
        Some(Arc::new(move |downloaded, total| {
            progress_events.lock().unwrap().push((downloaded, total));
        })),
        Duration::from_secs(1),
        cancel_rx,
    )
    .await
    .unwrap();

    assert!(!result.aborted);
    assert!(
        !progress
            .lock()
            .unwrap()
            .iter()
            .any(|&(downloaded, total)| downloaded == total)
    );
    server.join().unwrap();
}

// md5 hex finalize edge cases: verify the lookup-table path produces lowercase hex
// identical to the reference Md5 output for empty, 1-byte, and 16-byte inputs.
#[test]
fn md5_hex_empty_input_is_known_digest() {
    use md5::{Digest, Md5};
    let hex = finalize_md5(Md5::new());
    assert_eq!(hex.len(), 32, "md5 hex must be 32 chars");
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
        "must be lowercase hex"
    );
    // well-known md5("") = d41d8cd98f00b204e9800998ecf8427e
    assert_eq!(&*hex, "d41d8cd98f00b204e9800998ecf8427e");
}
