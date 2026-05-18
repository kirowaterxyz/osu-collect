use osu_downloader::http::create_download_client;

#[test]
fn create_download_client_succeeds() {
    assert!(create_download_client(None).is_ok());
}

#[test]
fn create_download_client_with_user_agent() {
    assert!(create_download_client(Some("test-agent".to_string())).is_ok());
}

#[cfg(feature = "collection")]
#[test]
fn create_api_client_succeeds() {
    use osu_downloader::http::create_api_client;
    assert!(create_api_client().is_ok());
}
