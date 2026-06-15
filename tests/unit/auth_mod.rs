use super::{
    LAZER_CLIENT_ID, LAZER_CLIENT_SECRET, LAZER_SCOPE, StoredAuth, X_API_VERSION,
    client_credentials_params, password_grant_params, refresh_params, token_request_failed,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Look up a form param's value by key.
fn value<'a>(params: &'a [(&'a str, &'a str)], key: &str) -> Option<&'a str> {
    params.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
}

#[test]
fn password_grant_body_includes_required_fields() {
    let params = password_grant_params("player", "hunter2");
    assert_eq!(value(&params, "grant_type"), Some("password"));
    assert_eq!(value(&params, "client_id"), Some(LAZER_CLIENT_ID));
    assert_eq!(value(&params, "client_secret"), Some(LAZER_CLIENT_SECRET));
    assert_eq!(value(&params, "username"), Some("player"));
    assert_eq!(value(&params, "password"), Some("hunter2"));
    // `scope=*` is what carries beatmap-download privilege.
    assert_eq!(value(&params, "scope"), Some(LAZER_SCOPE));
}

#[test]
fn lazer_constants_are_well_formed() {
    assert_eq!(LAZER_CLIENT_ID, "5");
    assert_eq!(LAZER_SCOPE, "*");
    // x-api-version must be a recent YYYYMMDD integer.
    assert_eq!(X_API_VERSION.len(), 8);
    assert!(X_API_VERSION.chars().all(|c| c.is_ascii_digit()));
}

#[test]
fn refresh_body_carries_scope_through() {
    // A `*` (lazer) token must refresh with `*`, not a narrower default.
    let params = refresh_params("5", "secret", "rt_abc", "*");
    assert_eq!(value(&params, "grant_type"), Some("refresh_token"));
    assert_eq!(value(&params, "refresh_token"), Some("rt_abc"));
    assert_eq!(value(&params, "scope"), Some("*"));
}

#[test]
fn client_credentials_body_includes_required_fields() {
    let params = client_credentials_params("42", "secret");
    assert_eq!(value(&params, "grant_type"), Some("client_credentials"));
    assert_eq!(value(&params, "scope"), Some("public"));
}

#[test]
fn token_request_error_omits_body() {
    let err = token_request_failed("login", reqwest::StatusCode::UNAUTHORIZED);
    let message = err.to_string();
    assert!(message.contains("login failed"));
    assert!(message.contains("401"));
    // The error must never leak submitted credentials or token material.
    assert!(!message.contains("access_token"));
    assert!(!message.contains("password"));
}

#[test]
fn has_lazer_scope_gates_official_mirror() {
    let mut auth = StoredAuth {
        client_id: LAZER_CLIENT_ID.into(),
        client_secret: LAZER_CLIENT_SECRET.into(),
        redirect_uri: String::new(),
        access_token: "tok".into(),
        refresh_token: Some("rt".into()),
        expires_at: 0,
        scopes: vec![LAZER_SCOPE.into()],
    };
    assert!(auth.has_lazer_scope(), "a `*` token must pass the gate");

    // An old browser-OAuth token (no `*`) must be rejected for the mirror.
    auth.scopes = vec!["public".into(), "identify".into()];
    assert!(!auth.has_lazer_scope());

    auth.scopes = vec![];
    assert!(
        !auth.has_lazer_scope(),
        "a scopeless token must be rejected"
    );
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
        client_id: LAZER_CLIENT_ID.into(),
        client_secret: LAZER_CLIENT_SECRET.into(),
        redirect_uri: String::new(),
        access_token: "access".into(),
        refresh_token: Some("refresh".into()),
        expires_at: 9999999999,
        scopes: vec![LAZER_SCOPE.into()],
    };

    let json = serde_json::to_string_pretty(&auth).unwrap();
    std::fs::write(&path, &json).unwrap();

    let loaded: StoredAuth =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    assert_eq!(loaded.access_token, "access");
    assert_eq!(loaded.refresh_token.as_deref(), Some("refresh"));
    assert!(loaded.scopes.contains(&"*".to_string()));
}
