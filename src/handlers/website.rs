use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};
use base64::Engine;
use hyper::{Method, StatusCode};
use serde::{Deserialize, Serialize};

use crate::db;
use crate::server::response::Response;
use crate::state::{SharedState, WebSession};

pub const SESSION_COOKIE_NAME: &str = "los_session";
pub const SESSION_AGE_SECS: u64 = 86400; // 24 hours

#[derive(Serialize)]
pub struct SessionResponse {
    pub csrf: String,
    pub user: Option<String>,
    pub profiles: Vec<String>,
    pub active: Option<String>,
    pub default: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    #[serde(default)]
    pub username: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub name: String,
}

#[derive(Serialize)]
pub struct SuccessResponse {
    pub ok: bool,
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn generate_token() -> String {
    let mut buf = [0u8; 32];
    let _ = getrandom::getrandom(&mut buf);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buf)
}

pub fn extract_cookie<'a>(cookie_header: &'a str, cookie_name: &str) -> Option<&'a str> {
    for pair in cookie_header.split(';') {
        let trimmed = pair.trim();
        if let Some((k, v)) = trimmed.split_once('=') {
            if k.trim() == cookie_name {
                return Some(v.trim());
            }
        }
    }
    None
}

pub fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn can_write(
    session: Option<&WebSession>,
    headers: &hyper::HeaderMap,
) -> bool {
    let session = match session {
        Some(s) => s,
        None => return false,
    };

    // Verify Origin or Referer matches request Host
    let host = headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("");
    let host_domain = host.split(':').next().unwrap_or("").to_lowercase();

    let origin_header = headers.get("origin").and_then(|v| v.to_str().ok());
    let referer_header = headers.get("referer").and_then(|v| v.to_str().ok());

    let origin_target = origin_header.or(referer_header);
    let origin_matches = match origin_target {
        Some(target) => {
            let without_proto = target.split("://").nth(1).unwrap_or(target);
            let domain_part = without_proto.split('/').next().unwrap_or("");
            let domain_name = domain_part.split(':').next().unwrap_or("").to_lowercase();
            domain_name == host_domain
                || (domain_name == "localhost" && host_domain == "127.0.0.1")
                || (domain_name == "127.0.0.1" && host_domain == "localhost")
        }
        None => false,
    };

    if !origin_matches {
        return false;
    }

    // Verify CSRF token
    let csrf_header = headers.get("x-csrf-token").and_then(|v| v.to_str().ok()).unwrap_or("");
    if csrf_header.is_empty() {
        return false;
    }

    constant_time_eq(csrf_header.as_bytes(), session.csrf_token.as_bytes())
}

pub fn validate_username(name: &str) -> Result<String, &'static str> {
    let trimmed = name.trim();
    if trimmed.is_empty() || trimmed.len() > 64 {
        return Err("Enter a username of 1–64 characters without slashes or control characters.");
    }
    if trimmed.chars().any(|c| (c as u32) < 32 || c == '/' || c == '\\') {
        return Err("Enter a username of 1–64 characters without slashes or control characters.");
    }
    Ok(trimmed.to_string())
}

fn json_ok<T: Serialize>(data: &T) -> Response {
    let val = serde_json::to_value(data).unwrap_or(serde_json::Value::Null);
    Response::json(&val).with_header("Cache-Control", "no-store")
}

fn json_error(status: StatusCode, message: &str) -> Response {
    let val = serde_json::json!({ "error": message });
    Response::json(&val).with_status(status).with_header("Cache-Control", "no-store")
}

pub async fn handle(
    state: SharedState,
    action: &str,
    _params: &HashMap<String, String>,
    method: &Method,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    match (method, action) {
        (&Method::GET, "session") => handle_session(state, headers).await,
        (&Method::POST, "login") => handle_login(state, headers, body).await,
        (&Method::POST, "logout") => handle_logout(state, headers).await,
        _ => json_error(StatusCode::NOT_FOUND, "Not found"),
    }
}

async fn handle_session(state: SharedState, headers: &hyper::HeaderMap) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let mut state_guard = state.write().await;

    // Prune expired sessions
    state_guard.web_sessions.retain(|_, s| s.expires > now);

    let (token, csrf_token, username, set_cookie) = match cookie_val.and_then(|t| state_guard.web_sessions.get(t).map(|s| (t.to_string(), s.clone()))) {
        Some((t, session)) => (t, session.csrf_token, session.username, false),
        None => {
            let new_token = generate_token();
            let new_csrf = generate_token();
            let new_session = WebSession {
                csrf_token: new_csrf.clone(),
                username: None,
                expires: now + SESSION_AGE_SECS,
            };
            state_guard.web_sessions.insert(new_token.clone(), new_session);
            (new_token, new_csrf, None, true)
        }
    };

    let active = state_guard.player.as_ref().map(|p| p.name.clone());

    // Fetch profile lists from DB
    let (profiles, default) = {
        let conn = state_guard.db.lock().await;
        let p_list = db::list_profiles(&conn).unwrap_or_default();
        let def = db::get_default_profile(&conn).unwrap_or_default();
        (p_list, def)
    };

    drop(state_guard);

    let mut resp = json_ok(&SessionResponse {
        csrf: csrf_token,
        user: username,
        profiles,
        active,
        default,
    });

    if set_cookie {
        let cookie_str = format!(
            "{}={}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}",
            SESSION_COOKIE_NAME, token, SESSION_AGE_SECS
        );
        resp = resp.with_header("Set-Cookie", &cookie_str);
    }

    resp
}

async fn handle_login(state: SharedState, headers: &hyper::HeaderMap, body: &[u8]) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let token = match cookie_val {
        Some(t) => t.to_string(),
        None => return json_error(StatusCode::FORBIDDEN, "Refresh the page and try signing in again."),
    };

    let mut state_guard = state.write().await;
    state_guard.web_sessions.retain(|_, s| s.expires > now);

    let can_write_ok = {
        let session = state_guard.web_sessions.get(&token);
        can_write(session, headers)
    };

    if !can_write_ok {
        return json_error(StatusCode::FORBIDDEN, "Refresh the page and try signing in again.");
    }

    let payload: LoginPayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "Invalid request payload."),
    };

    let username = match validate_username(&payload.username) {
        Ok(name) => name,
        Err(err) => return json_error(StatusCode::BAD_REQUEST, err),
    };

    // Ensure profile exists in database
    {
        let conn = state_guard.db.lock().await;
        if let Err(e) = db::ensure_profile(&conn, &username) {
            return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Database error: {}", e));
        }
    }

    if let Some(session) = state_guard.web_sessions.get_mut(&token) {
        session.username = Some(username.clone());
    }

    json_ok(&LoginResponse { name: username })
}

async fn handle_logout(state: SharedState, headers: &hyper::HeaderMap) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let token = match cookie_val {
        Some(t) => t.to_string(),
        None => return json_error(StatusCode::FORBIDDEN, "Refresh the page and try again."),
    };

    let mut state_guard = state.write().await;
    state_guard.web_sessions.retain(|_, s| s.expires > now);

    let can_write_ok = {
        let session = state_guard.web_sessions.get(&token);
        can_write(session, headers)
    };

    if !can_write_ok {
        return json_error(StatusCode::FORBIDDEN, "Refresh the page and try again.");
    }

    if let Some(session) = state_guard.web_sessions.get_mut(&token) {
        session.username = None;
    }

    json_ok(&SuccessResponse { ok: true })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::AppState;
    use hyper::header::HeaderValue;
    use hyper::HeaderMap;

    #[test]
    fn test_extract_cookie() {
        let header = "other=123; los_session=my_secure_token; theme=dark";
        assert_eq!(extract_cookie(header, "los_session"), Some("my_secure_token"));
        assert_eq!(extract_cookie(header, "other"), Some("123"));
        assert_eq!(extract_cookie(header, "theme"), Some("dark"));
        assert_eq!(extract_cookie(header, "missing"), None);
    }

    #[test]
    fn test_constant_time_eq() {
        assert!(constant_time_eq(b"token123", b"token123"));
        assert!(!constant_time_eq(b"token123", b"token124"));
        assert!(!constant_time_eq(b"token123", b"token12"));
    }

    #[test]
    fn test_validate_username() {
        assert_eq!(validate_username("Alice").unwrap(), "Alice");
        assert_eq!(validate_username("  Bob_123  ").unwrap(), "Bob_123");
        assert!(validate_username("").is_err());
        assert!(validate_username("a".repeat(65).as_str()).is_err());
        assert!(validate_username("user/name").is_err());
        assert!(validate_username("user\\name").is_err());
        assert!(validate_username("user\x00name").is_err());
    }

    #[test]
    fn test_can_write_verification() {
        let session = WebSession {
            csrf_token: "csrf_secret_123".to_string(),
            username: Some("Alice".to_string()),
            expires: 9999999999,
        };

        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:5000"));
        headers.insert("x-csrf-token", HeaderValue::from_static("csrf_secret_123"));

        assert!(can_write(Some(&session), &headers));

        // Mismatched CSRF token
        let mut bad_csrf = headers.clone();
        bad_csrf.insert("x-csrf-token", HeaderValue::from_static("wrong_token"));
        assert!(!can_write(Some(&session), &bad_csrf));

        // Cross-origin attack
        let mut bad_origin = headers.clone();
        bad_origin.insert("origin", HeaderValue::from_static("https://evil.com"));
        assert!(!can_write(Some(&session), &bad_origin));

        // Missing session
        assert!(!can_write(None, &headers));
    }

    #[tokio::test]
    async fn test_session_bootstrap_login_and_logout_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // 1. Initial GET /site/session without cookie
        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        let resp = handle_session(shared_state.clone(), &headers).await;
        assert_eq!(resp.status, StatusCode::OK);

        let cookie_header = resp.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("set-cookie")).expect("Set-Cookie header missing");
        let cookie_val = extract_cookie(&cookie_header.1, SESSION_COOKIE_NAME).expect("Session token in cookie");

        let body: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        let csrf = body["csrf"].as_str().unwrap().to_string();
        assert!(body["user"].is_null());

        // 2. Subsequent GET /site/session with cookie preserves session
        headers.insert("cookie", HeaderValue::from_str(&format!("{}={}", SESSION_COOKIE_NAME, cookie_val)).unwrap());
        let resp2 = handle_session(shared_state.clone(), &headers).await;
        assert_eq!(resp2.status, StatusCode::OK);
        let body2: serde_json::Value = serde_json::from_slice(&resp2.body).unwrap();
        assert_eq!(body2["csrf"].as_str().unwrap(), csrf);
        assert!(!resp2.headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("set-cookie")));

        // 3. Login without CSRF rejected
        headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:5000"));
        let bad_login = handle_login(shared_state.clone(), &headers, b"{\"username\":\"Alice\"}").await;
        assert_eq!(bad_login.status, StatusCode::FORBIDDEN);

        // 4. Valid login
        headers.insert("x-csrf-token", HeaderValue::from_str(&csrf).unwrap());
        let login_resp = handle_login(shared_state.clone(), &headers, b"{\"username\":\"Alice\"}").await;
        assert_eq!(login_resp.status, StatusCode::OK);
        let login_body: serde_json::Value = serde_json::from_slice(&login_resp.body).unwrap();
        assert_eq!(login_body["name"], "Alice");

        // Profile should now be in DB and session user should be "Alice"
        let resp3 = handle_session(shared_state.clone(), &headers).await;
        let body3: serde_json::Value = serde_json::from_slice(&resp3.body).unwrap();
        assert_eq!(body3["user"], "Alice");
        assert!(body3["profiles"].as_array().unwrap().iter().any(|p| p == "Alice"));

        // 5. Logout
        let logout_resp = handle_logout(shared_state.clone(), &headers).await;
        assert_eq!(logout_resp.status, StatusCode::OK);

        // Session user should now be null
        let resp4 = handle_session(shared_state.clone(), &headers).await;
        let body4: serde_json::Value = serde_json::from_slice(&resp4.body).unwrap();
        assert!(body4["user"].is_null());
    }

    #[tokio::test]
    async fn test_session_expiration_pruning() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        let mut app_state = AppState::new(conn, crate::types::config::Config::default());

        // Insert an expired session
        let expired_token = "expired_token_123";
        app_state.web_sessions.insert(expired_token.to_string(), WebSession {
            csrf_token: "old_csrf".to_string(),
            username: Some("OldUser".to_string()),
            expires: 100, // expired long ago
        });

        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        let mut headers = HeaderMap::new();
        headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        headers.insert("cookie", HeaderValue::from_str(&format!("{}={}", SESSION_COOKIE_NAME, expired_token)).unwrap());

        let resp = handle_session(shared_state.clone(), &headers).await;
        assert_eq!(resp.status, StatusCode::OK);

        // A new cookie should have been issued because old one was expired
        let cookie_header = resp.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("set-cookie")).expect("New cookie header");
        let new_token = extract_cookie(&cookie_header.1, SESSION_COOKIE_NAME).unwrap();
        assert_ne!(new_token, expired_token);

        let state_guard = shared_state.read().await;
        assert!(!state_guard.web_sessions.contains_key(expired_token));
    }
}
