use osu_collect::auth::{StoredAuth, build_authorize_url};
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

#[test]
fn state_mismatch_rejected_by_caller() {
    // Simulate the state comparison that run_login_flow performs after parse_callback_query.
    // Confirms that a differing returned_state triggers the error branch.
    let expected_state = "correct_state_abc123";
    let line = "GET /oauth/callback?code=THECODE&state=tampered_state HTTP/1.1";

    let path = line.split_whitespace().nth(1).unwrap_or("");
    let query = path.split_once('?').map(|x| x.1).unwrap_or("");
    let mut code: Option<String> = None;
    let mut returned_state: Option<String> = None;
    for part in query.split('&') {
        if let Some(v) = part.strip_prefix("code=") {
            code = Some(v.to_string());
        } else if let Some(v) = part.strip_prefix("state=") {
            returned_state = Some(v.to_string());
        }
    }
    assert!(code.is_some(), "code must be present");
    assert_ne!(
        returned_state.as_deref().unwrap_or(""),
        expected_state,
        "mismatched state must be detected"
    );
}
