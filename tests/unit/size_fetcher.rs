use osu_collect::download::size_fetcher::{check_availability_on_urls, check_mirror_availability};
use reqwest::Client;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::{Duration, timeout};

async fn start_server(
    handler: impl Fn(String) -> String + Send + 'static + Sync,
) -> (String, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base = format!("http://{}", addr);

    let handle = tokio::spawn(async move {
        let mut requests = 0u8;
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(pair) => pair,
                Err(_) => break,
            };
            requests = requests.saturating_add(1);

            let mut buf = [0u8; 1024];
            let read = socket.read(&mut buf).await.unwrap_or(0);
            if read == 0 {
                break;
            }
            let request = String::from_utf8_lossy(&buf[..read]).to_string();
            let response = handler(request);
            if socket.write_all(response.as_bytes()).await.is_err() {
                break;
            }
            if requests > 3 {
                break;
            }
        }
    });

    (base, handle)
}

#[tokio::test]
async fn availability_follows_redirect_then_succeeds() {
    let (base, handle) = start_server(|request| {
        if request.contains("/redirect/") {
            "HTTP/1.1 302 Found\r\nLocation: /final\r\nContent-Length: 0\r\n\r\n"
                .to_string()
        } else if request.contains("/final") {
            let mut body = vec![0x50, 0x4B, 0x03, 0x04];
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.append(&mut body);
            String::from_utf8(response).unwrap()
        } else {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
        }
    })
    .await;

    let redirect_template = format!("{}/redirect/{{id}}", base);
    let client = Client::new();
    let available = check_availability_on_urls(&client, 42, &[redirect_template.as_str()]).await;
    assert!(available);

    handle.abort();
}

#[tokio::test]
async fn probe_reads_only_zip_magic_prefix() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let stopped_early = Arc::new(AtomicBool::new(false));
    let stopped_early_ref = stopped_early.clone();

    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buf = [0u8; 1024];
        let _ = socket.read(&mut buf).await.unwrap();
        let header = "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: 1048576\r\n\r\n";
        socket.write_all(header.as_bytes()).await.unwrap();
        socket.write_all(&[0x50, 0x4B, 0x03, 0x04]).await.unwrap();
        for _ in 0..32 {
            tokio::time::sleep(Duration::from_millis(10)).await;
            if socket.write_all(&[b'x'; 8192]).await.is_err() {
                stopped_early_ref.store(true, Ordering::SeqCst);
                return;
            }
        }
    });

    let template = format!("{}/ok", base);
    let client = Client::new();
    let available = check_availability_on_urls(&client, 7, &[template.as_str()]).await;
    assert!(available);

    let _ = timeout(Duration::from_secs(2), handle).await;
    assert!(stopped_early.load(Ordering::SeqCst));
}

#[tokio::test]
async fn availability_short_circuits_when_other_mirrors_hang() {
    let (fast_base, fast_handle) = start_server(|_request| {
        let mut body = vec![0x50, 0x4B, 0x03, 0x04];
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.append(&mut body);
        String::from_utf8(response).unwrap()
    })
    .await;

    let slow_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let slow_addr = slow_listener.local_addr().unwrap();
    let slow_base = format!("http://{}", slow_addr);
    let slow_handle = tokio::spawn(async move {
        while let Ok((_socket, _)) = slow_listener.accept().await {
            tokio::time::sleep(Duration::from_secs(30)).await;
        }
    });

    let fast_template = format!("{}/d/{{id}}", fast_base);
    let slow_template = format!("{}/d/{{id}}", slow_base);
    let client = Client::new();
    let start = Instant::now();
    let available = check_availability_on_urls(
        &client,
        42,
        &[fast_template.as_str(), slow_template.as_str()],
    )
    .await;
    let elapsed = start.elapsed();

    assert!(
        available,
        "expected success once fast mirror returns ZIP magic"
    );
    assert!(
        elapsed < Duration::from_secs(5),
        "short-circuit failed: took {}ms (full probe timeout is 10s × retries)",
        elapsed.as_millis()
    );

    fast_handle.abort();
    slow_handle.abort();
}

#[tokio::test]
async fn availability_progress_reports_each_checked_map() {
    let (base, handle) = start_server(|_request| {
        let mut body = vec![0x50, 0x4B, 0x03, 0x04];
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.append(&mut body);
        String::from_utf8(response).unwrap()
    })
    .await;

    let template = format!("{}/d/{{id}}", base);
    let client = Client::new();
    let mut progress = Vec::new();
    let result = check_mirror_availability(
        &client,
        &[1, 2, 3],
        &[template.as_str()],
        |checked, total| progress.push((checked, total)),
    )
    .await;

    assert_eq!(result.available.len(), 3);
    assert_eq!(progress, vec![(1, 3), (2, 3), (3, 3)]);

    handle.abort();
}

#[tokio::test]
async fn probe_retries_after_server_error() {
    let served_error = Arc::new(AtomicBool::new(false));
    let served_error_ref = served_error.clone();
    let (base, handle) = start_server(move |request| {
        if !served_error_ref.swap(true, Ordering::SeqCst) {
            "HTTP/1.1 500 Internal Server Error\r\nContent-Length: 0\r\n\r\n".to_string()
        } else if request.contains("/ok") {
            let mut body = vec![0x50, 0x4B, 0x03, 0x04];
            let mut response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
                body.len()
            )
            .into_bytes();
            response.append(&mut body);
            String::from_utf8(response).unwrap()
        } else {
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
        }
    })
    .await;

    let template = format!("{}/ok", base);
    let client = Client::new();
    let available = check_availability_on_urls(&client, 7, &[template.as_str()]).await;
    assert!(available);

    handle.abort();
}
