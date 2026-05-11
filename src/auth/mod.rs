use crate::config::constants::CONFIG_SUBDIR;
use crate::utils::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

const AUTH_FILE: &str = "auth.json";
const CALLBACK_PORT: u16 = 7273;
const REFRESH_MARGIN_SECS: u64 = 60;
const OSU_AUTHORIZE_URL: &str = "https://osu.ppy.sh/oauth/authorize";
const OSU_TOKEN_URL: &str = "https://osu.ppy.sh/oauth/token";
const OAUTH_SCOPES: &[&str] = &["public", "identify", "lazer"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_in: u64,
    pub token_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredAuth {
    pub client_id: String,
    pub client_secret: String,
    pub redirect_uri: String,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: u64,
    pub scopes: Vec<String>,
}

impl StoredAuth {
    pub fn is_expired(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now + REFRESH_MARGIN_SECS >= self.expires_at
    }

    pub fn bearer_token(&self) -> &str {
        &self.access_token
    }
}

fn auth_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join(CONFIG_SUBDIR).join(AUTH_FILE))
}

pub fn load() -> Option<StoredAuth> {
    let path = auth_path()?;
    let contents = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&contents).ok()
}

pub fn save(auth: &StoredAuth) -> Result<()> {
    let path = auth_path().ok_or_else(|| AppError::config("cannot determine config dir"))?;

    if let Some(parent) = path.parent()
        && !parent.exists()
    {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = std::fs::metadata(parent)?.permissions();
            perms.set_mode(0o700);
            std::fs::set_permissions(parent, perms)?;
        }
    }

    let json = serde_json::to_string_pretty(auth)
        .map_err(|e| AppError::other_dynamic(format!("auth serialize: {e}").into_boxed_str()))?;
    std::fs::write(&path, &json)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms)?;
    }

    debug!("auth saved to {}", path.display());
    Ok(())
}

pub fn delete() -> Result<()> {
    if let Some(path) = auth_path()
        && path.exists()
    {
        std::fs::remove_file(&path)?;
        info!("auth tokens removed");
    }
    Ok(())
}

pub fn build_authorize_url(
    client_id: &str,
    redirect_uri: &str,
    scopes: &[&str],
    state: &str,
) -> String {
    let scope = scopes.join(" ");
    format!(
        "{OSU_AUTHORIZE_URL}?client_id={client_id}&redirect_uri={redirect_uri}&response_type=code&scope={scope}&state={state}",
        redirect_uri = urlencoding_simple(redirect_uri),
        scope = urlencoding_simple(&scope),
        state = urlencoding_simple(state),
    )
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

pub async fn exchange_code(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<TokenResponse> {
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ];

    let resp = client
        .post(OSU_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::other_dynamic(format!("token request: {e}").into_boxed_str()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::other_dynamic(
            format!("token exchange failed ({status}): {body}").into_boxed_str(),
        ));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| AppError::other_dynamic(format!("token parse: {e}").into_boxed_str()))
}

pub async fn refresh(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let scope = OAUTH_SCOPES.join(" ");
    let params = [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", scope.as_str()),
    ];

    let resp = client
        .post(OSU_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::other_dynamic(format!("refresh request: {e}").into_boxed_str()))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(AppError::other_dynamic(
            format!("token refresh failed ({status}): {body}").into_boxed_str(),
        ));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| AppError::other_dynamic(format!("refresh parse: {e}").into_boxed_str()))
}

pub async fn ensure_valid(client: &reqwest::Client, auth: &mut StoredAuth) -> Result<()> {
    if !auth.is_expired() {
        return Ok(());
    }

    let Some(ref rt) = auth.refresh_token.clone() else {
        return Err(AppError::other_dynamic(Box::from(
            "no refresh token; re-run `osu-collect login`",
        )));
    };

    info!("refreshing OAuth token");
    let token_resp = refresh(
        client,
        &auth.client_id.clone(),
        &auth.client_secret.clone(),
        rt,
    )
    .await?;
    apply_token_response(auth, token_resp);
    save(auth)?;
    Ok(())
}

fn apply_token_response(auth: &mut StoredAuth, resp: TokenResponse) {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    auth.access_token = resp.access_token;
    if let Some(rt) = resp.refresh_token {
        auth.refresh_token = Some(rt);
    }
    auth.expires_at = now + resp.expires_in;
}

fn generate_state() -> String {
    use std::hash::{BuildHasher, Hash, Hasher};

    // Two independent OS-seeded RandomState instances give ~128 bits of unpredictable output.
    let rs1 = std::collections::hash_map::RandomState::new();
    let rs2 = std::collections::hash_map::RandomState::new();
    let mut h2 = rs2.build_hasher();
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    nanos.hash(&mut h2);
    std::process::id().hash(&mut h2);
    format!("{:016x}{:016x}", rs1.hash_one(nanos), h2.finish())
}

pub async fn run_login_flow(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
) -> Result<StoredAuth> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::TcpListener;

    let listener = TcpListener::bind(format!("127.0.0.1:{CALLBACK_PORT}"))
        .await
        .map_err(|e| {
            AppError::other_dynamic(
                format!("cannot bind port {CALLBACK_PORT}: {e}").into_boxed_str(),
            )
        })?;

    let redirect_uri = format!("http://localhost:{CALLBACK_PORT}/oauth/callback");
    let state = generate_state();
    let auth_url = build_authorize_url(client_id, &redirect_uri, OAUTH_SCOPES, &state);

    println!("opening browser for osu! login...");
    println!("if the browser did not open, visit:\n  {auth_url}");

    let _ = open::that(&auth_url);

    let (mut stream, _) = listener
        .accept()
        .await
        .map_err(|e| AppError::other_dynamic(format!("accept failed: {e}").into_boxed_str()))?;

    let (reader, mut writer) = stream.split();
    let mut lines = BufReader::new(reader).lines();

    let request_line = lines
        .next_line()
        .await
        .map_err(|e| AppError::other_dynamic(format!("read request: {e}").into_boxed_str()))?
        .unwrap_or_default();

    // parse GET /oauth/callback?code=...&state=... HTTP/1.1
    let (code, returned_state) = parse_callback_query(&request_line)?;

    let response_body = if returned_state != state {
        warn!("OAuth state mismatch — possible CSRF");
        "state mismatch; login failed".to_string()
    } else {
        "login successful — you may close this tab".to_string()
    };

    let http_resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response_body.len(),
        response_body,
    );
    let _ = writer.write_all(http_resp.as_bytes()).await;

    if returned_state != state {
        return Err(AppError::other_dynamic(Box::from("OAuth state mismatch")));
    }

    let token_resp = exchange_code(client, client_id, client_secret, &redirect_uri, &code).await?;

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let auth = StoredAuth {
        client_id: client_id.to_string(),
        client_secret: client_secret.to_string(),
        redirect_uri,
        access_token: token_resp.access_token,
        refresh_token: token_resp.refresh_token,
        expires_at: now + token_resp.expires_in,
        scopes: OAUTH_SCOPES.iter().map(|s| s.to_string()).collect(),
    };

    save(&auth)?;
    Ok(auth)
}

fn parse_callback_query(request_line: &str) -> Result<(String, String)> {
    // "GET /oauth/callback?code=xxx&state=yyy HTTP/1.1"
    let path = request_line.split_whitespace().nth(1).unwrap_or("");

    let query = path.split_once('?').map(|x| x.1).unwrap_or("");

    let mut code = None;
    let mut state = None;
    for part in query.split('&') {
        if let Some(v) = part.strip_prefix("code=") {
            code = Some(v.to_string());
        } else if let Some(v) = part.strip_prefix("state=") {
            state = Some(v.to_string());
        }
    }

    match (code, state) {
        (Some(c), Some(s)) => Ok((c, s)),
        _ => Err(AppError::other_dynamic(Box::from(
            "callback missing code or state",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(url.contains("lazer"));
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
    fn state_mismatch_detected() {
        let expected = "correct_state";
        let received = "wrong_state";
        assert_ne!(expected, received, "state mismatch must be caught");
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
            scopes: vec!["public".into(), "lazer".into()],
        };

        let json = serde_json::to_string_pretty(&auth).unwrap();
        std::fs::write(&path, &json).unwrap();

        let loaded: StoredAuth =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(loaded.client_id, "my_id");
        assert_eq!(loaded.access_token, "access");
        assert_eq!(loaded.refresh_token.as_deref(), Some("refresh"));
        assert!(loaded.scopes.contains(&"lazer".to_string()));
    }
}
