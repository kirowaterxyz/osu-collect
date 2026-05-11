use osu_collect::download::size_fetcher::check_availability_on_urls;
use reqwest::Client;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
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
