//! Web accounts: register, log in, log out, current session.
//!
//! Passwords are hashed with Argon2id. Sessions are random tokens in an
//! HttpOnly, SameSite=Lax cookie; only their SHA-256 is stored. Every
//! cookie-authenticated POST must carry `X-Davra: 1`, which a cross-site form
//! cannot send, so CSRF needs no extra token.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use axum::extract::{ConnectInfo, FromRequestParts, State};
use axum::http::header::{COOKIE, SET_COOKIE, USER_AGENT};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::AppState;

pub const COOKIE_NAME: &str = "davra_session";
const SESSION_DAYS: i64 = 30;
const MIN_PASSWORD: usize = 10;
const MAX_PASSWORD: usize = 200;
const LOGIN_LIMIT: u32 = 10;
const REGISTER_LIMIT: u32 = 5;
const WINDOW: Duration = Duration::from_secs(15 * 60);

#[derive(Serialize, Clone, Debug)]
pub struct UserView {
    pub id: Uuid,
    pub name: String,
    pub email: String,
}

#[derive(Serialize)]
pub struct UserResponse {
    pub user: UserView,
}

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub name: String,
    pub email: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Fixed-window attempt counter, keyed by IP or e-mail.
#[derive(Default)]
pub struct RateLimiter {
    hits: Mutex<HashMap<String, (Instant, u32)>>,
}

impl RateLimiter {
    /// Records an attempt; returns false when the key is over `limit` in the window.
    pub fn allow(&self, key: &str, limit: u32) -> bool {
        let mut hits = self.hits.lock().expect("rate limiter lock");
        let now = Instant::now();
        if hits.len() > 50_000 {
            hits.retain(|_, (start, _)| now.duration_since(*start) < WINDOW);
        }
        let entry = hits.entry(key.to_string()).or_insert((now, 0));
        if now.duration_since(entry.0) >= WINDOW {
            *entry = (now, 0);
        }
        entry.1 += 1;
        entry.1 <= limit
    }
}

pub fn require_csrf(headers: &HeaderMap) -> AppResult<()> {
    match headers.get("x-davra").and_then(|v| v.to_str().ok()) {
        Some("1") => Ok(()),
        _ => Err(AppError::Forbidden("missing X-Davra header".into())),
    }
}

/// Client IP: the trusted proxy's header when the operator configured one and
/// it holds a valid address (first entry of a list), else the socket peer.
fn client_ip(trusted: Option<&HeaderName>, headers: &HeaderMap, peer: SocketAddr) -> String {
    trusted
        .and_then(|h| headers.get(h))
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .and_then(|s| s.trim().parse::<std::net::IpAddr>().ok())
        .map(|ip| ip.to_string())
        .unwrap_or_else(|| peer.ip().to_string())
}

fn rate_limited() -> AppError {
    AppError::RateLimited("too many attempts, try again in a few minutes".into())
}

fn valid_email(e: &str) -> bool {
    let e = e.trim();
    let Some((local, domain)) = e.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && e.len() <= 254
        && !e.chars().any(|c| c.is_whitespace() || c.is_control())
}

fn hash_password(pw: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Ok(Argon2::default()
        .hash_password(pw.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("hashing failed: {e}"))?
        .to_string())
}

fn verify_password(pw: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .map(|h| Argon2::default().verify_password(pw.as_bytes(), &h).is_ok())
        .unwrap_or(false)
}

/// A real hash of a random password, so unknown e-mails cost the same time.
static DUMMY_HASH: std::sync::LazyLock<String> =
    std::sync::LazyLock::new(|| hash_password("dummy-password-for-timing").expect("dummy hash"));

fn session_token() -> String {
    let mut b = [0u8; 32];
    rand::rng().fill_bytes(&mut b);
    format!("ses_{}", hex::encode(b))
}

fn token_hash(t: &str) -> Vec<u8> {
    Sha256::digest(t.as_bytes()).to_vec()
}

fn cookie(state: &AppState, value: &str, max_age: i64) -> HeaderValue {
    let secure = if state.cookie_secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{COOKIE_NAME}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .expect("valid cookie header")
}

fn read_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == COOKIE_NAME)
        .map(|(_, v)| v.to_string())
        .filter(|v| v.starts_with("ses_"))
}

async fn start_session(
    state: &AppState,
    user_id: Uuid,
    headers: &HeaderMap,
) -> AppResult<HeaderValue> {
    let token = session_token();
    let ua = headers
        .get(USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.chars().take(300).collect::<String>());
    sqlx::query(
        "insert into sessions (user_id, token_hash, user_agent, expires_at) values ($1, $2, $3, $4)",
    )
    .bind(user_id)
    .bind(token_hash(&token))
    .bind(ua)
    .bind(Utc::now() + chrono::Duration::days(SESSION_DAYS))
    .execute(&state.pool)
    .await?;
    Ok(cookie(state, &token, SESSION_DAYS * 86_400))
}

fn with_cookie(status: StatusCode, cookie: HeaderValue, body: impl Serialize) -> Response {
    let mut resp = (status, Json(serde_json::to_value(body).unwrap_or_default())).into_response();
    resp.headers_mut().append(SET_COOKIE, cookie);
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

pub async fn register(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> AppResult<Response> {
    require_csrf(&headers)?;
    if !state.registration_open {
        return Err(AppError::Forbidden(
            "registration is closed on this hub".into(),
        ));
    }
    if !state.limiter.allow(
        &format!(
            "reg:{}",
            client_ip(state.client_ip_header.as_ref(), &headers, peer)
        ),
        REGISTER_LIMIT,
    ) {
        return Err(rate_limited());
    }
    let name = req.name.trim();
    let email = req.email.trim();
    if name.is_empty() || name.chars().count() > 100 {
        return Err(AppError::Field(
            "name",
            "enter your name (up to 100 characters)".into(),
        ));
    }
    if !valid_email(email) {
        return Err(AppError::Field(
            "email",
            "enter a valid e-mail address".into(),
        ));
    }
    let n = req.password.chars().count();
    if !(MIN_PASSWORD..=MAX_PASSWORD).contains(&n) {
        return Err(AppError::Field(
            "password",
            format!("password must be {MIN_PASSWORD} to {MAX_PASSWORD} characters"),
        ));
    }
    let pw = req.password.clone();
    let hash = tokio::task::spawn_blocking(move || hash_password(&pw))
        .await
        .map_err(|e| AppError::Internal(e.into()))??;
    let row: Option<(Uuid,)> = sqlx::query_as(
        "insert into users (email, name, password_hash) values ($1, $2, $3) \
         on conflict (email_lower) do nothing returning id",
    )
    .bind(email)
    .bind(name)
    .bind(&hash)
    .fetch_optional(&state.pool)
    .await?;
    let Some((id,)) = row else {
        return Err(AppError::Conflict(
            "an account with this e-mail already exists".into(),
        ));
    };
    crate::db::audit(
        &state.pool,
        None,
        None,
        "user.register",
        Some(&id.to_string()),
        serde_json::json!({}),
    )
    .await?;
    let c = start_session(&state, id, &headers).await?;
    Ok(with_cookie(
        StatusCode::CREATED,
        c,
        UserResponse {
            user: UserView {
                id,
                name: name.into(),
                email: email.into(),
            },
        },
    ))
}

pub async fn login(
    State(state): State<AppState>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> AppResult<Response> {
    require_csrf(&headers)?;
    let email = req.email.trim().to_lowercase();
    let ip_ok = state.limiter.allow(
        &format!(
            "login-ip:{}",
            client_ip(state.client_ip_header.as_ref(), &headers, peer)
        ),
        LOGIN_LIMIT * 3,
    );
    let email_ok = state
        .limiter
        .allow(&format!("login-email:{email}"), LOGIN_LIMIT);
    if !ip_ok || !email_ok {
        return Err(rate_limited());
    }
    let row: Option<(Uuid, String, String, String)> =
        sqlx::query_as("select id, name, email, password_hash from users where email_lower = $1")
            .bind(&email)
            .fetch_optional(&state.pool)
            .await?;
    let (hash, user) = match row {
        Some((id, name, email, hash)) => (hash, Some(UserView { id, name, email })),
        None => (DUMMY_HASH.clone(), None),
    };
    let pw = req.password.clone();
    let ok = tokio::task::spawn_blocking(move || verify_password(&pw, &hash))
        .await
        .map_err(|e| AppError::Internal(e.into()))?;
    let user = match (ok, user) {
        (true, Some(u)) => u,
        _ => return Err(AppError::InvalidCredentials),
    };
    sqlx::query("update users set last_login_at = now() where id = $1")
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    let c = start_session(&state, user.id, &headers).await?;
    Ok(with_cookie(StatusCode::OK, c, UserResponse { user }))
}

pub async fn logout(State(state): State<AppState>, headers: HeaderMap) -> AppResult<Response> {
    require_csrf(&headers)?;
    if let Some(t) = read_cookie(&headers) {
        sqlx::query("delete from sessions where token_hash = $1")
            .bind(token_hash(&t))
            .execute(&state.pool)
            .await?;
    }
    let mut resp = StatusCode::NO_CONTENT.into_response();
    resp.headers_mut().append(SET_COOKIE, cookie(&state, "", 0));
    Ok(resp)
}

/// The signed-in web user, from the session cookie.
pub struct SessionUser(pub UserView);

impl FromRequestParts<AppState> for SessionUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = read_cookie(&parts.headers).ok_or(AppError::Unauthorized)?;
        let row: Option<(Uuid, String, String, Uuid)> = sqlx::query_as(
            "update sessions s set last_seen_at = now() from users u \
             where s.token_hash = $1 and s.expires_at > now() and u.id = s.user_id \
             returning u.id, u.name, u.email, s.id",
        )
        .bind(token_hash(&token))
        .fetch_optional(&state.pool)
        .await?;
        let (id, name, email, _) = row.ok_or(AppError::Unauthorized)?;
        Ok(SessionUser(UserView { id, name, email }))
    }
}

pub async fn session(SessionUser(user): SessionUser) -> Response {
    let mut resp = Json(UserResponse { user }).into_response();
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        HeaderValue::from_static("no-store"),
    );
    resp
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_ip_trusts_header_only_when_configured() {
        let peer: SocketAddr = "192.0.2.7:5000".parse().unwrap();
        let cf = HeaderName::from_static("cf-connecting-ip");
        let mut h = HeaderMap::new();
        h.insert("cf-connecting-ip", HeaderValue::from_static("203.0.113.9"));
        assert_eq!(client_ip(None, &h, peer), "192.0.2.7");
        assert_eq!(client_ip(Some(&cf), &h, peer), "203.0.113.9");
        h.insert(
            "cf-connecting-ip",
            HeaderValue::from_static(" 203.0.113.9, 10.0.0.1"),
        );
        assert_eq!(client_ip(Some(&cf), &h, peer), "203.0.113.9");
        h.insert("cf-connecting-ip", HeaderValue::from_static("not-an-ip"));
        assert_eq!(client_ip(Some(&cf), &h, peer), "192.0.2.7");
        assert_eq!(client_ip(Some(&cf), &HeaderMap::new(), peer), "192.0.2.7");
    }

    #[test]
    fn emails() {
        assert!(valid_email("ali@qitmir.uz"));
        assert!(!valid_email("ali@localhost"));
        assert!(!valid_email("no-at.uz"));
        assert!(!valid_email("a b@x.uz"));
    }

    #[test]
    fn passwords_roundtrip() {
        let h = hash_password("correct horse battery").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password("correct horse battery", &h));
        assert!(!verify_password("wrong", &h));
        assert!(!verify_password("x", "not a hash"));
    }

    #[test]
    fn limiter_blocks_after_limit() {
        let l = RateLimiter::default();
        for _ in 0..3 {
            assert!(l.allow("k", 3));
        }
        assert!(!l.allow("k", 3));
        assert!(l.allow("other", 3));
    }

    #[test]
    fn reads_session_cookie() {
        let mut h = HeaderMap::new();
        h.insert(
            COOKIE,
            HeaderValue::from_static("theme=dark; davra_session=ses_abc; x=1"),
        );
        assert_eq!(read_cookie(&h).as_deref(), Some("ses_abc"));
        let mut h = HeaderMap::new();
        h.insert(COOKIE, HeaderValue::from_static("davra_session=evil"));
        assert_eq!(read_cookie(&h), None);
    }
}
