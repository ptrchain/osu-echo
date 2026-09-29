use std::collections::HashMap;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use base64::Engine;
use hyper::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as TokioMutex;

use crate::db;
use crate::server::response::Response;
use crate::state::{SharedState, WebSession};
use crate::types::mods::Mods;
use crate::types::profile::ProfileDetails;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicBeatmap {
    pub title: String,
    pub artist: String,
    pub version: String,
    pub creator: String,
    pub beatmap_id: i64,
    pub beatmapset_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicScore {
    pub id: String,
    pub pp: f64,
    pub acc: f64,
    pub score: i64,
    pub max_combo: i32,
    pub time: i64,
    pub n300: i32,
    pub n100: i32,
    pub n50: i32,
    pub nmiss: i32,
    pub grade: String,
    pub mods: String,
    pub beatmap: PublicBeatmap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MostPlayedEntry {
    pub count: i32,
    pub beatmap: PublicBeatmap,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileResponse {
    pub name: String,
    pub mode: String,
    pub pp: i32,
    pub acc: f64,
    pub playcount: i32,
    pub total_score: i64,
    pub ranked_score: i64,
    pub total_hits: i64,
    pub max_combo: i32,
    pub grades: HashMap<String, i32>,
    pub active: bool,
    pub details: ProfileDetails,
    pub country_rank: Option<i32>,
    pub global_rank: Option<i32>,
    pub rank_history: Vec<(String, i32)>,
    pub top: Vec<PublicScore>,
    pub recent: Vec<PublicScore>,
    pub last_play: Option<i64>,
    pub performance_history: Vec<(String, f64)>,
    pub play_history: Vec<(String, i32)>,
    pub most_played: Vec<MostPlayedEntry>,
    pub recent_24h: Vec<PublicScore>,
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

static DAILY_CACHE: TokioMutex<Option<HashMap<i32, (Instant, Option<i32>)>>> =
    TokioMutex::const_new(None);
static DAILY_LAST_REQUEST: TokioMutex<Option<Instant>> = TokioMutex::const_new(None);

pub async fn get_daily_rank(
    http: &reqwest::Client,
    api_key_opt: Option<&str>,
    pp: i32,
) -> Option<i32> {
    let api_key = api_key_opt?;
    if api_key.trim().is_empty() || pp <= 0 {
        return None;
    }

    let mut cache_guard = DAILY_CACHE.lock().await;
    let cache = cache_guard.get_or_insert_with(HashMap::new);

    let now = Instant::now();
    if let Some((cached_time, cached_rank)) = cache.get(&pp) {
        if now.duration_since(*cached_time).as_secs() < 300 {
            return *cached_rank;
        }
    }

    // Rate limiting: sleep if last request was less than 1.1s ago
    let mut last_req_guard = DAILY_LAST_REQUEST.lock().await;
    if let Some(last_time) = *last_req_guard {
        let elapsed = now.duration_since(last_time);
        if elapsed.as_millis() < 1100 {
            tokio::time::sleep(std::time::Duration::from_millis(1100 - elapsed.as_millis() as u64)).await;
        }
    }
    *last_req_guard = Some(Instant::now());
    drop(last_req_guard);

    let url = format!("https://osudaily.net/api/pp.php?k={}&t=pp&v={}&m=0", api_key, pp);
    let mut rank: Option<i32> = None;
    if let Ok(resp) = http.get(&url).timeout(std::time::Duration::from_secs(8)).send().await {
        if resp.status().is_success() {
            if let Ok(json) = resp.json::<serde_json::Value>().await {
                if let Some(r) = json.get("rank").and_then(|v| v.as_i64()) {
                    if r > 0 {
                        rank = Some(r as i32);
                    }
                }
            }
        }
    }

    cache.insert(pp, (Instant::now(), rank));
    rank
}

fn score_grade(s: &db::ScoreWithBeatmap) -> &'static str {
    let g = crate::utils::get_grade(
        s.mode as u8,
        s.n300,
        s.n100,
        s.n50,
        s.ngeki,
        s.nkatu,
        s.nmiss,
        s.mods,
        s.acc,
    );
    match g {
        "SSH" => "XH",
        "SS" => "X",
        other => other,
    }
}

fn to_public_score(s: &db::ScoreWithBeatmap) -> PublicScore {
    let mods_str = if let Some(ref ms) = s.mods_str {
        if !ms.is_empty() {
            ms.clone()
        } else {
            Mods::from_bits_truncate(s.mods).short_name()
        }
    } else {
        Mods::from_bits_truncate(s.mods).short_name()
    };

    PublicScore {
        id: s.id.to_string(),
        pp: (s.pp * 100.0).round() / 100.0,
        acc: (s.acc * 100.0).round() / 100.0,
        score: s.score,
        max_combo: s.max_combo,
        time: s.time,
        n300: s.n300,
        n100: s.n100,
        n50: s.n50,
        nmiss: s.nmiss,
        grade: score_grade(s).to_string(),
        mods: mods_str,
        beatmap: PublicBeatmap {
            title: s.title.clone().unwrap_or_else(|| "Unknown beatmap".to_string()),
            artist: s.artist.clone().unwrap_or_else(|| "unknown artist".to_string()),
            version: s.version.clone().unwrap_or_else(|| "Unknown difficulty".to_string()),
            creator: s.creator.clone().unwrap_or_default(),
            beatmap_id: s.beatmap_id.unwrap_or(0),
            beatmapset_id: s.beatmapset_id.unwrap_or(0),
        },
    }
}

pub async fn handle(
    state: SharedState,
    action: &str,
    params: &HashMap<String, String>,
    method: &Method,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    match (method, action) {
        (&Method::GET, "session") => handle_session(state, headers).await,
        (&Method::GET, "profile") => handle_profile(state, params).await,
        (&Method::POST, "login") => handle_login(state, headers, body).await,
        (&Method::POST, "logout") => handle_logout(state, headers).await,
        _ => json_error(StatusCode::NOT_FOUND, "Not found"),
    }
}

async fn handle_profile(state: SharedState, params: &HashMap<String, String>) -> Response {
    let name = match params.get("name") {
        Some(n) if !n.trim().is_empty() => n.trim(),
        _ => return json_error(StatusCode::BAD_REQUEST, "Profile username is required."),
    };

    let mode_str = params.get("mode").map(|s| s.as_str()).unwrap_or("vn");
    if !["vn", "rx", "ap"].contains(&mode_str) {
        return json_error(StatusCode::BAD_REQUEST, "Unknown mode.");
    }

    let state_guard = state.read().await;
    let conn = state_guard.db.lock().await;

    if !db::profile_exists(&conn, name).unwrap_or(false) {
        return json_error(
            StatusCode::NOT_FOUND,
            "This local profile does not exist. Sign in with its username to create it.",
        );
    }

    let details = db::get_profile_details(&conn, name).unwrap_or_default();
    let scores = db::get_player_scores_with_beatmaps(&conn, name).unwrap_or_default();

    let is_active = state_guard.player.as_ref().map_or(false, |p| p.name == name);
    let http_client = state_guard.http.clone();
    let daily_key = state_guard.config.osu_daily_api_key.clone();

    drop(conn);
    drop(state_guard);

    // Filter scores by mode:
    let (rx_bit, ap_bit) = (
        Mods::RELAX.bits() as u32,
        Mods::AUTOPILOT.bits() as u32,
    );

    let mode_scores: Vec<&db::ScoreWithBeatmap> = scores
        .iter()
        .filter(|s| match mode_str {
            "rx" => (s.mods & rx_bit) != 0,
            "ap" => (s.mods & ap_bit) != 0,
            _ => (s.mods & (rx_bit | ap_bit)) == 0,
        })
        .collect();

    // Ranked plays for PP, accuracy, and grades
    let mut ranked: Vec<&&db::ScoreWithBeatmap> = mode_scores
        .iter()
        .filter(|s| s.bmap_status == "ranked" || s.bmap_status == "approved")
        .collect();
    ranked.sort_by(|a, b| b.pp.partial_cmp(&a.pp).unwrap_or(std::cmp::Ordering::Equal));

    // Deduplicate top plays by md5 taking best PP per map
    let mut seen_md5 = std::collections::HashSet::new();
    let mut top_scores: Vec<&&db::ScoreWithBeatmap> = Vec::new();
    for s in &ranked {
        if seen_md5.insert(&s.md5) {
            top_scores.push(s);
            if top_scores.len() == 100 {
                break;
            }
        }
    }

    let mut weighted_pp = 0.0;
    let mut weighted_acc = 0.0;
    let mut weight_sum = 0.0;
    for (i, s) in top_scores.iter().enumerate() {
        let weight = 0.95_f64.powi(i as i32);
        weighted_pp += s.pp * weight;
        weighted_acc += s.acc * weight;
        weight_sum += weight;
    }
    if !ranked.is_empty() {
        weighted_pp += 416.6667 * (1.0 - 0.9994_f64.powi(ranked.len() as i32));
    }
    let calculated_pp = weighted_pp.round() as i32;
    let calculated_acc = if weight_sum > 0.0 { weighted_acc / weight_sum } else { 0.0 };

    // Grade tallies across deduplicated ranked plays
    let mut grade_counts: HashMap<String, i32> = [
        ("XH".to_string(), 0),
        ("X".to_string(), 0),
        ("SH".to_string(), 0),
        ("S".to_string(), 0),
        ("A".to_string(), 0),
    ].into_iter().collect();

    let mut seen_for_grades = std::collections::HashSet::new();
    for s in &ranked {
        if seen_for_grades.insert(&s.md5) {
            let g = score_grade(s);
            if let Some(cnt) = grade_counts.get_mut(g) {
                *cnt += 1;
            }
        }
    }

    // Best score per map for ranked_score
    let mut best_scores_per_map: HashMap<&str, i64> = HashMap::new();
    for s in &ranked {
        let entry = best_scores_per_map.entry(&s.md5).or_insert(0);
        if s.score > *entry {
            *entry = s.score;
        }
    }
    let ranked_score: i64 = best_scores_per_map.values().sum();
    let total_score: i64 = mode_scores.iter().map(|s| s.score).sum();
    let total_hits: i64 = mode_scores.iter().map(|s| (s.n300 + s.n100 + s.n50) as i64).sum();
    let max_combo: i32 = mode_scores.iter().map(|s| s.max_combo).max().unwrap_or(0);
    let playcount = mode_scores.len() as i32;

    // Recent plays (ordered by time DESC)
    let recent_public: Vec<PublicScore> = mode_scores.iter().take(100).map(|s| to_public_score(s)).collect();
    let top_public: Vec<PublicScore> = top_scores.iter().map(|s| to_public_score(s)).collect();

    let now_ts = now_secs() as i64;
    let recent_24h_public: Vec<PublicScore> = mode_scores
        .iter()
        .filter(|s| s.time >= now_ts - 86400)
        .take(100)
        .map(|s| to_public_score(s))
        .collect();

    let last_play = mode_scores.first().map(|s| s.time);

    // Most played maps
    let mut map_counts: HashMap<&str, (i32, &db::ScoreWithBeatmap)> = HashMap::new();
    for s in &mode_scores {
        let entry = map_counts.entry(&s.md5).or_insert((0, s));
        entry.0 += 1;
    }
    let mut most_played: Vec<MostPlayedEntry> = map_counts
        .into_iter()
        .map(|(_, (count, rep))| MostPlayedEntry {
            count,
            beatmap: to_public_score(rep).beatmap,
        })
        .collect();
    most_played.sort_by(|a, b| b.count.cmp(&a.count));
    most_played.truncate(100);

    // Play history (by YYYY-MM)
    let mut month_counts: HashMap<String, i32> = HashMap::new();
    for s in &mode_scores {
        if s.time > 0 {
            if let Some(dt) = chrono::DateTime::from_timestamp(s.time, 0) {
                let month = dt.format("%Y-%m").to_string();
                *month_counts.entry(month).or_insert(0) += 1;
            }
        }
    }
    let mut play_history: Vec<(String, i32)> = month_counts.into_iter().collect();
    play_history.sort_by(|a, b| a.0.cmp(&b.0));

    // Performance history (daily peak PP)
    let mut day_peaks: HashMap<String, f64> = HashMap::new();
    for s in &mode_scores {
        if s.time > 0 && s.pp > 0.0 {
            if let Some(dt) = chrono::DateTime::from_timestamp(s.time, 0) {
                let day = dt.format("%Y-%m-%d").to_string();
                let entry = day_peaks.entry(day).or_insert(0.0);
                if s.pp > *entry {
                    *entry = s.pp;
                }
            }
        }
    }
    let mut perf_history: Vec<(String, f64)> = day_peaks.into_iter().collect();
    perf_history.sort_by(|a, b| a.0.cmp(&b.0));
    if perf_history.len() > 90 {
        perf_history = perf_history.split_off(perf_history.len() - 90);
    }

    // Rank snapshot and history
    let mode_id = match mode_str {
        "rx" => 100,
        "ap" => 200,
        _ => 0,
    };

    let global_rank = get_daily_rank(&http_client, daily_key.as_deref(), calculated_pp).await;

    // Save rank snapshot if valid rank obtained
    let state_guard = state.read().await;
    let conn = state_guard.db.lock().await;
    if let Some(rank) = global_rank {
        let _ = db::record_rank_snapshot(&conn, name, mode_id, rank);
    }
    let rank_history = db::get_rank_history(&conn, name, mode_id, 90).unwrap_or_default();
    drop(conn);
    drop(state_guard);

    json_ok(&ProfileResponse {
        name: name.to_string(),
        mode: mode_str.to_string(),
        pp: calculated_pp,
        acc: (calculated_acc * 100.0).round() / 100.0,
        playcount,
        total_score,
        ranked_score,
        total_hits,
        max_combo,
        grades: grade_counts,
        active: is_active,
        details,
        country_rank: None,
        global_rank,
        rank_history,
        top: top_public,
        recent: recent_public,
        last_play,
        performance_history: perf_history,
        play_history,
        most_played,
        recent_24h: recent_24h_public,
    })
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

    #[tokio::test]
    async fn test_handle_profile_not_found_and_invalid_mode() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // Missing profile name
        let params_empty = HashMap::new();
        let resp_empty = handle_profile(shared_state.clone(), &params_empty).await;
        assert_eq!(resp_empty.status, StatusCode::BAD_REQUEST);

        // Non-existent profile
        let mut params = HashMap::new();
        params.insert("name".to_string(), "GhostUser".to_string());
        let resp_nf = handle_profile(shared_state.clone(), &params).await;
        assert_eq!(resp_nf.status, StatusCode::NOT_FOUND);

        // Invalid mode
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "Alice").unwrap();
        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        params.insert("name".to_string(), "Alice".to_string());
        params.insert("mode".to_string(), "invalid_mode".to_string());
        let resp_bad_mode = handle_profile(shared_state.clone(), &params).await;
        assert_eq!(resp_bad_mode.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_handle_profile_data_and_mode_filtering() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "Alice").unwrap();

        // Insert beatmaps
        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.file_md5 = "map_1".to_string();
        bmap.title = "Song One".to_string();
        bmap.artist = "Artist A".to_string();
        bmap.version = "Hard".to_string();
        bmap.creator = "Mapper A".to_string();
        bmap.beatmap_id = 1001;
        bmap.beatmapset_id = 2001;
        crate::db::insert_beatmap(&conn, &bmap).unwrap();

        let mut bmap2 = crate::types::beatmap::Beatmap::blank();
        bmap2.file_md5 = "map_2".to_string();
        bmap2.title = "Song Two".to_string();
        bmap2.artist = "Artist B".to_string();
        bmap2.version = "Insane".to_string();
        bmap2.creator = "Mapper B".to_string();
        bmap2.beatmap_id = 1002;
        bmap2.beatmapset_id = 2002;
        crate::db::insert_beatmap(&conn, &bmap2).unwrap();

        // 1. Standard ranked score on map_1 (300pp, SS / X)
        let s1 = crate::types::score::Score {
            mode: 0,
            md5: "map_1".to_string(),
            name: "Alice".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 1000000,
            max_combo: 500,
            perfect: true,
            mods: 0, // NM
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(300.0),
            replay_md5: None,
            replay_frames: None,
            mods_str: None,
            scoreid: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        crate::db::insert_score(&conn, &s1, "ranked").unwrap();

        // 2. Standard ranked score on map_2 with HD (200pp, SSH / XH)
        let s2 = crate::types::score::Score {
            mode: 0,
            md5: "map_2".to_string(),
            name: "Alice".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 1050000,
            max_combo: 500,
            perfect: true,
            mods: 8, // HD
            time: 1700086400,
            acc: Some(100.0),
            pp: Some(200.0),
            replay_md5: None,
            replay_frames: None,
            mods_str: Some("HD".to_string()),
            scoreid: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        crate::db::insert_score(&conn, &s2, "ranked").unwrap();

        // 3. Relax score (should only show in rx mode)
        let s_rx = crate::types::score::Score {
            mode: 0,
            md5: "map_1".to_string(),
            name: "Alice".to_string(),
            n300: 290,
            n100: 10,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 950000,
            max_combo: 480,
            perfect: false,
            mods: 128, // RX
            time: 1700172800,
            acc: Some(98.5),
            pp: Some(400.0),
            replay_md5: None,
            replay_frames: None,
            mods_str: Some("RX".to_string()),
            scoreid: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        crate::db::insert_score(&conn, &s_rx, "ranked").unwrap();

        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // Test VN mode
        let mut params_vn = HashMap::new();
        params_vn.insert("name".to_string(), "Alice".to_string());
        params_vn.insert("mode".to_string(), "vn".to_string());
        let resp_vn = handle_profile(shared_state.clone(), &params_vn).await;
        assert_eq!(resp_vn.status, StatusCode::OK);

        let data: ProfileResponse = serde_json::from_slice(&resp_vn.body).unwrap();
        assert_eq!(data.name, "Alice");
        assert_eq!(data.mode, "vn");
        assert_eq!(data.playcount, 2);
        assert!(data.pp > 0);
        assert_eq!(data.top.len(), 2);
        assert_eq!(data.top[0].beatmap.title, "Song One");
        assert_eq!(data.top[0].grade, "X");
        assert_eq!(data.top[1].beatmap.title, "Song Two");
        assert_eq!(data.top[1].grade, "XH");
        assert_eq!(*data.grades.get("X").unwrap(), 1);
        assert_eq!(*data.grades.get("XH").unwrap(), 1);
        assert_eq!(data.ranked_score, 2050000);
        assert_eq!(data.total_score, 2050000);
        assert_eq!(data.most_played.len(), 2);

        // Test RX mode
        let mut params_rx = HashMap::new();
        params_rx.insert("name".to_string(), "Alice".to_string());
        params_rx.insert("mode".to_string(), "rx".to_string());
        let resp_rx = handle_profile(shared_state.clone(), &params_rx).await;
        assert_eq!(resp_rx.status, StatusCode::OK);

        let data_rx: ProfileResponse = serde_json::from_slice(&resp_rx.body).unwrap();
        assert_eq!(data_rx.playcount, 1);
        assert_eq!(data_rx.top.len(), 1);
        assert_eq!(data_rx.top[0].mods, "RX");
    }
}
