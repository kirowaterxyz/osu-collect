use super::{StoredAuth, build_authorize_url};
use std::time::{SystemTime, UNIX_EPOCH};

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn make_auth(expires_at: u64) -> StoredAuth {
    StoredAuth {
        client_id: "1".into(),
        client_secret: "s".into(),
        redirect_uri: "http://localhost:7273/oauth/callback".into(),
        access_token: "tok".into(),
        refresh_token: Some("rtok".into()),
        expires_at,
        scopes: vec!["public".into(), "identify".into()],
    }
}

#[test]
fn authorize_url_required_params() {
    let url = build_authorize_url(
        "99",
        "http://localhost:7273/oauth/callback",
        &["public", "identify"],
        "mystate",
    );
    assert!(url.starts_with("https://osu.ppy.sh/oauth/authorize"));
    assert!(url.contains("client_id=99"));
    assert!(url.contains("response_type=code"));
    assert!(url.contains("state=mystate"));
    assert!(url.contains("public+identify"));
    assert!(!url.contains("lazer"));
}

#[test]
fn token_not_expired_when_far_future() {
    let auth = make_auth(now_secs() + 3600);
    assert!(!auth.is_expired());
}

#[test]
fn token_expired_when_in_past() {
    let auth = make_auth(now_secs() - 1);
    assert!(auth.is_expired());
}

#[test]
fn token_expired_within_margin() {
    // expires in 30s — within the 60s refresh margin
    let auth = make_auth(now_secs() + 30);
    assert!(auth.is_expired());
}

#[test]
fn token_persistence_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("auth.json");

    let auth = make_auth(9999999999);
    let json = serde_json::to_string_pretty(&auth).unwrap();
    std::fs::write(&path, &json).unwrap();

    let loaded: StoredAuth =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();

    assert_eq!(loaded.client_id, "1");
    assert_eq!(loaded.access_token, "tok");
    assert_eq!(loaded.refresh_token.as_deref(), Some("rtok"));
    assert!(loaded.scopes.contains(&"public".to_string()));
    assert!(loaded.scopes.contains(&"identify".to_string()));
    assert!(!loaded.scopes.contains(&"lazer".to_string()));
}
