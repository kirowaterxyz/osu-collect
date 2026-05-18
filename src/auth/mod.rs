use crate::config::constants::CONFIG_SUBDIR;
use crate::utils::{AppError, Result};
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{debug, info, warn};

pub fn bundled_credentials() -> Option<(&'static str, &'static str)> {
    let id = option_env!("OSU_CLIENT_ID")?;
    let secret = option_env!("OSU_CLIENT_SECRET")?;
    if id.is_empty() || secret.is_empty() {
        return None;
    }
    Some((id, secret))
}

const AUTH_FILE: &str = "auth.json";
const CALLBACK_PORT: u16 = 7273;
const REFRESH_MARGIN_SECS: u64 = 60;
const OSU_AUTHORIZE_URL: &str = "https://osu.ppy.sh/oauth/authorize";
const OSU_TOKEN_URL: &str = "https://osu.ppy.sh/oauth/token";
pub const OAUTH_SCOPES: &[&str] = &["public", "identify"];
const LOGIN_SUCCESS_PAGE: &str = include_str!("pages/login_success.html");
const LOGIN_FAILURE_PAGE: &str = include_str!("pages/login_failure.html");

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

    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, &json)?;
    {
        use std::fs::File;
        File::open(&tmp)?.sync_all()?;
    }
    std::fs::rename(&tmp, &path)?;

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

pub fn authorization_code_params<'a>(
    client_id: &'a str,
    client_secret: &'a str,
    redirect_uri: &'a str,
    code: &'a str,
) -> [(&'a str, &'a str); 5] {
    [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("code", code),
        ("grant_type", "authorization_code"),
        ("redirect_uri", redirect_uri),
    ]
}

pub async fn exchange_code(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    redirect_uri: &str,
    code: &str,
) -> Result<TokenResponse> {
    let params = authorization_code_params(client_id, client_secret, redirect_uri, code);

    let resp = client
        .post(OSU_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::other_dynamic(format!("token request: {e}").into_boxed_str()))?;

    if !resp.status().is_success() {
        return Err(token_request_failed("token exchange", resp.status()));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| AppError::other_dynamic(format!("token parse: {e}").into_boxed_str()))
}

pub fn token_request_failed(operation: &str, status: reqwest::StatusCode) -> AppError {
    AppError::other_dynamic(format!("{operation} failed ({status})").into_boxed_str())
}

pub fn refresh_params<'a>(
    client_id: &'a str,
    client_secret: &'a str,
    refresh_token: &'a str,
    scope: &'a str,
) -> [(&'a str, &'a str); 5] {
    [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
        ("scope", scope),
    ]
}

pub async fn refresh(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
    refresh_token: &str,
) -> Result<TokenResponse> {
    let scope = OAUTH_SCOPES.join(" ");
    let params = refresh_params(client_id, client_secret, refresh_token, &scope);

    let resp = client
        .post(OSU_TOKEN_URL)
        .form(&params)
        .send()
        .await
        .map_err(|e| AppError::other_dynamic(format!("refresh request: {e}").into_boxed_str()))?;

    if !resp.status().is_success() {
        return Err(token_request_failed("token refresh", resp.status()));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| AppError::other_dynamic(format!("refresh parse: {e}").into_boxed_str()))
}

pub fn client_credentials_params<'a>(
    client_id: &'a str,
    client_secret: &'a str,
) -> [(&'a str, &'a str); 4] {
    [
        ("client_id", client_id),
        ("client_secret", client_secret),
        ("grant_type", "client_credentials"),
        ("scope", "public"),
    ]
}

pub async fn client_credentials(
    client: &reqwest::Client,
    client_id: &str,
    client_secret: &str,
) -> Result<TokenResponse> {
    let params = client_credentials_params(client_id, client_secret);

    request_token(client, OSU_TOKEN_URL, &params).await
}

async fn request_token(
    client: &reqwest::Client,
    token_url: &str,
    params: &[(&str, &str)],
) -> Result<TokenResponse> {
    let resp = client
        .post(token_url)
        .form(params)
        .send()
        .await
        .map_err(|e| AppError::other_dynamic(format!("token request: {e}").into_boxed_str()))?;

    if !resp.status().is_success() {
        return Err(token_request_failed("token request", resp.status()));
    }

    resp.json::<TokenResponse>()
        .await
        .map_err(|e| AppError::other_dynamic(format!("token parse: {e}").into_boxed_str()))
}

pub async fn ensure_valid(client: &reqwest::Client, auth: &mut StoredAuth) -> Result<()> {
    if !auth.is_expired() {
        return Ok(());
    }

    let token_resp = if let Some(refresh_token) = auth.refresh_token.as_deref() {
        info!("refreshing OAuth token");
        refresh(client, &auth.client_id, &auth.client_secret, refresh_token).await?
    } else {
        info!("refreshing OAuth token with client credentials");
        client_credentials(client, &auth.client_id, &auth.client_secret).await?
    };
    apply_token_response(auth, token_resp);
    let snap = auth.clone();
    tokio::task::spawn_blocking(move || save(&snap))
        .await
        .map_err(|e| {
            AppError::other_dynamic(format!("save task panicked: {e}").into_boxed_str())
        })??;
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

    open::that(&auth_url)
        .map_err(|e| AppError::other_dynamic(format!("open browser: {e}").into_boxed_str()))?;

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

    let (code, returned_state) = parse_callback_query(&request_line)?;

    let response_body = if returned_state != state {
        warn!("OAuth state mismatch (possible CSRF): expected {state}, got {returned_state}");
        LOGIN_FAILURE_PAGE
    } else {
        LOGIN_SUCCESS_PAGE
    };

    let http_resp = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
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

pub fn parse_callback_query(request_line: &str) -> Result<(String, String)> {
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
