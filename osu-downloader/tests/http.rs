use super::create_download_client;

#[test]
fn create_download_client_succeeds() {
    assert!(create_download_client(None).is_ok());
}

#[test]
fn create_download_client_with_user_agent() {
    assert!(create_download_client(Some("test-agent".to_string())).is_ok());
}

#[cfg(any(feature = "collection", feature = "size-fetch"))]
#[test]
fn create_api_client_succeeds() {
    use super::create_api_client;
    assert!(create_api_client().is_ok());
}
