use osu_collect::auth::{
    OAUTH_SCOPES, StoredAuth, authorization_code_params, build_authorize_url,
    client_credentials_params, parse_callback_query, refresh_params, token_request_failed,
};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn authorize_url_contains_required_params() {
    let url = build_authorize_url(
        "42",
        "http://localhost:7273/oauth/callback",
        OAUTH_SCOPES,
        "abc123",
    );
    assert!(url.contains("client_id=42"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("state=abc123"));
    assert!(url.contains("public+identify"));
    assert!(!url.contains("lazer"));
}

#[test]
fn parse_callback_query_ok() {
    let line = "GET /oauth/callback?code=THECODE&state=THESTATE HTTP/1.1";
    let (code, state) = parse_callback_query(line).unwrap();
    assert_eq!(code, "THECODE");
    assert_eq!(state, "THESTATE");
}

#[test]
fn parse_callback_query_missing_fields() {
    let line = "GET /oauth/callback?code=only HTTP/1.1";
    assert!(parse_callback_query(line).is_err());
}

#[test]
fn token_request_error_omits_body() {
    let err = token_request_failed("token refresh", reqwest::StatusCode::UNAUTHORIZED);
    let message = err.to_string();

    assert!(message.contains("token refresh failed"));
    assert!(message.contains("401"));
    assert!(!message.contains("access_token"));
    assert!(!message.contains("invalid_client"));
    assert!(!message.contains("secret"));
}

#[test]
fn authorization_code_body_includes_required_fields() {
    let params = authorization_code_params("42", "secret", "http://localhost/callback", "code");

    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "client_id" && *value == "42")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "client_secret" && *value == "secret")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "grant_type" && *value == "authorization_code")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "code" && *value == "code")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "redirect_uri" && *value == "http://localhost/callback")
    );
}

#[test]
fn refresh_body_includes_required_fields() {
    let params = refresh_params("42", "secret", "rt_abc", "public identify");

    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "client_id" && *value == "42")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "client_secret" && *value == "secret")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "grant_type" && *value == "refresh_token")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "refresh_token" && *value == "rt_abc")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "scope" && *value == "public identify")
    );
}

#[test]
fn client_credentials_body_includes_required_fields() {
    let params = client_credentials_params("42", "secret");

    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "client_id" && *value == "42")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "client_secret" && *value == "secret")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "grant_type" && *value == "client_credentials")
    );
    assert!(
        params
            .iter()
            .any(|(key, value)| *key == "scope" && *value == "public")
    );
}

#[test]
fn state_mismatch_detected() {
    let expected = "correct_state_abc123";
    let line = "GET /oauth/callback?code=THECODE&state=tampered_state HTTP/1.1";

    let (code, returned_state) = parse_callback_query(line).unwrap();
    assert_eq!(code, "THECODE");
    assert_ne!(returned_state, expected);
}

#[test]
fn stored_auth_expiry() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let fresh = StoredAuth {
        client_id: String::new(),
        client_secret: String::new(),
        redirect_uri: String::new(),
        access_token: "tok".into(),
        refresh_token: None,
        expires_at: now + 3600,
        scopes: vec![],
    };
    assert!(!fresh.is_expired());

    let stale = StoredAuth {
        expires_at: now - 1,
        ..fresh
    };
    assert!(stale.is_expired());
}

#[test]
fn token_persistence_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");

    let auth = StoredAuth {
        client_id: "my_id".into(),
        client_secret: "my_secret".into(),
        redirect_uri: "http://localhost:7273/oauth/callback".into(),
        access_token: "access".into(),
        refresh_token: Some("refresh".into()),
        expires_at: 9999999999,
        scopes: vec!["public".into(), "identify".into()],
    };

    let json = serde_json::to_string_pretty(&auth).unwrap();
    std::fs::write(&path, &json).unwrap();

    let loaded: StoredAuth =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(loaded.client_id, "my_id");
    assert_eq!(loaded.access_token, "access");
    assert_eq!(loaded.refresh_token.as_deref(), Some("refresh"));
    assert!(loaded.scopes.contains(&"public".to_string()));
    assert!(loaded.scopes.contains(&"identify".to_string()));
    assert!(!loaded.scopes.contains(&"lazer".to_string()));
}
