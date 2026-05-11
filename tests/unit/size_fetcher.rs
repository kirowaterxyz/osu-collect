use osu_collect::download::size_fetcher::check_availability_on_urls;
use reqwest::Client;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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
    let bytes_sent = Arc::new(AtomicUsize::new(0));
    let bytes_sent_ref = bytes_sent.clone();
    let (base, handle) = start_server(move |_| {
        let mut body = vec![b'x'; 1024 * 1024];
        body[..4].copy_from_slice(&[0x50, 0x4B, 0x03, 0x04]);
        bytes_sent_ref.store(body.len(), Ordering::SeqCst);
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(&body);
        String::from_utf8_lossy(&response).into_owned()
    })
    .await;

    let template = format!("{}/ok", base);
    let client = Client::new();
    let available = check_availability_on_urls(&client, 7, &[template.as_str()]).await;
    assert!(available);
    assert!(bytes_sent.load(Ordering::SeqCst) > 4);

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
