use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use base64::Engine;
use hyper::{Method, StatusCode};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex as TokioMutex;

use crate::db;
use crate::server::assets::EmbeddedAssets;
use crate::server::response::Response;
use crate::state::{SharedState, WebSession};
use crate::types::mods::Mods;
use crate::types::profile::ProfileDetails;
use crate::utils;

pub const SESSION_COOKIE_NAME: &str = "los_session";
pub const SESSION_AGE_SECS: u64 = 86400; // 24 hours

#[derive(Deserialize)]
pub struct SettingsPayload {
    pub username: Option<String>,
    #[serde(default)]
    pub reset_avatar: bool,
    pub details: Option<ProfileDetails>,
    pub avatar: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct SessionResponse {
    pub csrf: String,
    pub user: Option<String>,
    pub profiles: Vec<String>,
    pub active: Option<String>,
    pub default: Option<String>,
    #[serde(default)]
    pub avatar_version: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginPayload {
    #[serde(default)]
    pub username: String,
}

#[derive(Serialize, Deserialize)]
pub struct LoginResponse {
    pub name: String,
}

#[derive(Serialize, Deserialize)]
pub struct SuccessResponse {
    pub ok: bool,
}

#[derive(Deserialize)]
pub struct DeleteScorePayload {
    #[serde(deserialize_with = "deserialize_id")]
    pub id: i64,
}

fn default_true() -> bool {
    true
}

#[derive(Deserialize)]
pub struct ImportOfficialPayload {
    pub query: String,
    #[serde(default)]
    pub apply: bool,
    #[serde(default = "default_true")]
    pub import_avatar: bool,
    #[serde(default = "default_true")]
    pub import_bio: bool,
    #[serde(default = "default_true")]
    pub import_details: bool,
}

#[derive(Serialize, Deserialize)]
pub struct ImportOfficialResponse {
    pub ok: bool,
    pub official_username: String,
    pub official_id: i64,
    pub details: ProfileDetails,
    pub avatar_data_url: Option<String>,
    pub avatar_updated: bool,
    pub details_updated: bool,
    pub message: String,
}

fn deserialize_id<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct IdVisitor;
    impl<'de> serde::de::Visitor<'de> for IdVisitor {
        type Value = i64;
        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("an integer or string containing an integer")
        }
        fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E> {
            Ok(v)
        }
        fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
            Ok(v as i64)
        }
        fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
        where
            E: serde::de::Error,
        {
            v.parse::<i64>().map_err(serde::de::Error::custom)
        }
    }
    deserializer.deserialize_any(IdVisitor)
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
    #[serde(default)]
    pub has_replay: bool,
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
    #[serde(default)]
    pub pinned: Vec<PublicScore>,
    pub last_play: Option<i64>,
    #[serde(default)]
    pub first_play: Option<i64>,
    pub performance_history: Vec<(String, f64)>,
    pub play_history: Vec<(String, i32)>,
    pub most_played: Vec<MostPlayedEntry>,
    pub recent_24h: Vec<PublicScore>,
    #[serde(default)]
    pub medals: Vec<crate::types::medal::UserMedalDisplay>,
    #[serde(default)]
    pub rank_highest: Option<crate::db::RankHighest>,
    #[serde(default)]
    pub avatar_version: Option<String>,
}

pub fn compute_avatar_version(avatar_url: Option<&str>) -> String {
    match avatar_url {
        Some(url) => {
            if let Some(path) = utils::is_path(url) {
                if let Ok(metadata) = std::fs::metadata(&path) {
                    if let Ok(modified) = metadata.modified() {
                        let dur = modified
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default();
                        return format!("{}-{}", dur.as_secs(), dur.subsec_nanos());
                    }
                }
                if let Some(fname) = path.file_name().and_then(|n| n.to_str()) {
                    return fname.to_string();
                }
            }
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            std::hash::Hash::hash(url, &mut hasher);
            format!("{:x}", std::hash::Hasher::finish(&hasher))
        }
        None => "default".to_string(),
    }
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

pub fn generate_hex_token(nbytes: usize) -> String {
    let mut buf = vec![0u8; nbytes];
    let _ = getrandom::getrandom(&mut buf);
    buf.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn validate_image_format_and_dimensions(bytes: &[u8]) -> Result<&'static str, &'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        if bytes.len() < 24 {
            return Err("Invalid or truncated PNG image.");
        }
        if &bytes[12..16] != b"IHDR" {
            return Err("Invalid PNG IHDR header.");
        }
        let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
        let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err("Image dimensions must not exceed 4096x4096 pixels.");
        }
        return Ok("png");
    }

    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        let mut idx = 2;
        let mut found_dim = false;
        while idx + 4 < bytes.len() {
            if bytes[idx] != 0xFF {
                idx += 1;
                continue;
            }
            while idx < bytes.len() && bytes[idx] == 0xFF {
                idx += 1;
            }
            if idx >= bytes.len() {
                break;
            }
            let marker = bytes[idx];
            idx += 1;
            if marker == 0xD9 || marker == 0xDA {
                break;
            }
            if (0xD0..=0xD7).contains(&marker) || marker == 0x01 || marker == 0x00 {
                continue;
            }
            if idx + 2 > bytes.len() {
                break;
            }
            let seg_len = u16::from_be_bytes([bytes[idx], bytes[idx + 1]]) as usize;
            if seg_len < 2 || idx + seg_len > bytes.len() {
                break;
            }
            let is_sof = matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF);
            if is_sof && seg_len >= 7 {
                let height = u16::from_be_bytes([bytes[idx + 3], bytes[idx + 4]]) as u32;
                let width = u16::from_be_bytes([bytes[idx + 5], bytes[idx + 6]]) as u32;
                if width == 0 || height == 0 || width > 4096 || height > 4096 {
                    return Err("Image dimensions must not exceed 4096x4096 pixels.");
                }
                found_dim = true;
                break;
            }
            idx += seg_len;
        }
        if !found_dim && bytes.len() < 24 {
            return Err("Invalid or truncated JPEG image.");
        }
        return Ok("jpg");
    }

    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        if bytes.len() < 10 {
            return Err("Invalid or truncated GIF image.");
        }
        let width = u16::from_le_bytes([bytes[6], bytes[7]]) as u32;
        let height = u16::from_le_bytes([bytes[8], bytes[9]]) as u32;
        if width == 0 || height == 0 || width > 4096 || height > 4096 {
            return Err("Image dimensions must not exceed 4096x4096 pixels.");
        }
        return Ok("gif");
    }

    if bytes.len() >= 16 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        if &bytes[12..16] == b"VP8 " && bytes.len() >= 30 {
            let width = (u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF) as u32;
            let height = (u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF) as u32;
            if width > 4096 || height > 4096 {
                return Err("Image dimensions must not exceed 4096x4096 pixels.");
            }
        } else if &bytes[12..16] == b"VP8L" && bytes.len() >= 25 && bytes[20] == 0x2F {
            let width = 1 + (((bytes[22] as u32 & 0x3F) << 8) | bytes[21] as u32);
            let height = 1 + (((bytes[24] as u32 & 0xF) << 10) | ((bytes[23] as u32) << 2) | ((bytes[22] as u32 & 0xC0) >> 6));
            if width > 4096 || height > 4096 {
                return Err("Image dimensions must not exceed 4096x4096 pixels.");
            }
        } else if &bytes[12..16] == b"VP8X" && bytes.len() >= 30 {
            let width = 1 + (bytes[24] as u32 | ((bytes[25] as u32) << 8) | ((bytes[26] as u32) << 16));
            let height = 1 + (bytes[27] as u32 | ((bytes[28] as u32) << 8) | ((bytes[29] as u32) << 16));
            if width > 4096 || height > 4096 {
                return Err("Image dimensions must not exceed 4096x4096 pixels.");
            }
        }
        return Ok("webp");
    }

    Err("Choose a PNG, JPEG, GIF or WebP up to 2 MB.")
}

pub fn parse_and_validate_avatar(data_str: &str) -> Result<(&'static str, Vec<u8>), &'static str> {
    let raw_b64 = if let Some((_prefix, payload)) = data_str.split_once("base64,") {
        payload
    } else if data_str.starts_with("data:") {
        return Err("Invalid data URL format");
    } else {
        data_str
    };

    let cleaned: String = raw_b64.chars().filter(|c| !c.is_ascii_whitespace()).collect();
    if cleaned.is_empty() {
        return Err("Choose an image to upload.");
    }
    if cleaned.len() > 4 * 1024 * 1024 {
        return Err("Image exceeds maximum size of 2 MB.");
    }

    let bytes = base64::engine::general_purpose::STANDARD
        .decode(cleaned.as_bytes())
        .or_else(|_| base64::engine::general_purpose::URL_SAFE.decode(cleaned.as_bytes()))
        .map_err(|_| "Failed to decode base64 image data.")?;

    if bytes.is_empty() {
        return Err("Choose an image to upload.");
    }
    if bytes.len() > 2 * 1024 * 1024 {
        return Err("Image exceeds maximum size of 2 MB.");
    }

    let ext = validate_image_format_and_dimensions(&bytes)?;
    Ok((ext, bytes))
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

pub fn decode_html_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            let mut entity = String::new();
            while let Some(&next_c) = chars.peek() {
                if next_c == ';' || entity.len() >= 10 || next_c.is_whitespace() {
                    break;
                }
                entity.push(chars.next().unwrap());
            }
            let has_semicolon = if chars.peek() == Some(&';') {
                chars.next();
                true
            } else {
                false
            };
            match entity.as_str() {
                "quot" if has_semicolon => out.push('"'),
                "amp" if has_semicolon => out.push('&'),
                "lt" if has_semicolon => out.push('<'),
                "gt" if has_semicolon => out.push('>'),
                "apos" | "#039" | "#39" if has_semicolon => out.push('\''),
                _ => {
                    out.push('&');
                    out.push_str(&entity);
                    if has_semicolon {
                        out.push(';');
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn safe_truncate(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

#[derive(Debug, Clone, Default)]
pub struct OfficialUserParsed {
    pub id: i64,
    pub username: String,
    pub avatar_url: Option<String>,
    pub country_code: Option<String>,
    pub location: Option<String>,
    pub playstyle: Vec<String>,
    pub raw_bio: Option<String>,
}

pub fn parse_official_userpage_html(html: &str) -> Result<OfficialUserParsed, String> {
    let marker_double = "data-initial-data=\"";
    let marker_single = "data-initial-data='";

    let (raw_data, _quote) = if let Some(pos) = html.find(marker_double) {
        let start = pos + marker_double.len();
        let end = html[start..]
            .find('"')
            .ok_or_else(|| "Malformed HTML: unterminated data-initial-data attribute".to_string())?;
        (&html[start..start + end], '"')
    } else if let Some(pos) = html.find(marker_single) {
        let start = pos + marker_single.len();
        let end = html[start..]
            .find('\'')
            .ok_or_else(|| "Malformed HTML: unterminated data-initial-data attribute".to_string())?;
        (&html[start..start + end], '\'')
    } else {
        return Err("Could not locate profile initial data in official osu! page".to_string());
    };

    let decoded = decode_html_entities(raw_data);
    let root: serde_json::Value = serde_json::from_str(&decoded)
        .map_err(|e| format!("Failed to parse profile JSON: {}", e))?;

    let user = if root.get("user").is_some() {
        &root["user"]
    } else {
        &root
    };

    let id = user.get("id").and_then(|v| v.as_i64()).unwrap_or(0);
    let username = user.get("username").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if username.is_empty() {
        return Err("Official user profile did not contain a valid username".to_string());
    }

    let avatar_url = user.get("avatar_url").and_then(|v| v.as_str()).map(|s| s.to_string());
    let country_code = user.get("country_code").and_then(|v| v.as_str()).map(|s| s.to_uppercase());
    let location = user.get("location").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut playstyle = Vec::new();
    if let Some(arr) = user.get("playstyle").and_then(|v| v.as_array()) {
        for item in arr {
            if let Some(s) = item.as_str() {
                let normalized = match s.to_ascii_lowercase().as_str() {
                    "mouse" => "Mouse",
                    "keyboard" => "Keyboard",
                    "tablet" => "Tablet",
                    "touch" | "touchscreen" => "Touchscreen",
                    "controller" => "Controller",
                    _ => continue,
                };
                if !playstyle.contains(&normalized.to_string()) {
                    playstyle.push(normalized.to_string());
                }
            }
        }
    }

    let raw_bio = user
        .get("page")
        .and_then(|p| p.get("raw"))
        .and_then(|r| r.as_str())
        .map(|s| s.to_string());

    Ok(OfficialUserParsed {
        id,
        username,
        avatar_url,
        country_code,
        location,
        playstyle,
        raw_bio,
    })
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
                || (domain_name.ends_with(".localhost") && host_domain.ends_with(".localhost"))
                || (domain_name.ends_with(".localhost") && (host_domain == "localhost" || host_domain == "127.0.0.1"))
                || ((domain_name == "localhost" || domain_name == "127.0.0.1") && host_domain.ends_with(".localhost"))
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

fn json_ok<T: Serialize + ?Sized>(data: &T) -> Response {
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

static COUNTRY_RANK_CACHE: TokioMutex<Option<HashMap<(String, i32), (Instant, Option<i32>)>>> =
    TokioMutex::const_new(None);
static OAUTH_TOKEN: TokioMutex<Option<(String, Instant)>> = TokioMutex::const_new(None);

async fn fetch_oauth_token(
    http: &reqwest::Client,
    client_id: u64,
    client_secret: &str,
    token_guard: &mut tokio::sync::MutexGuard<'_, Option<(String, Instant)>>,
) -> Option<String> {
    let payload = serde_json::json!({
        "client_id": client_id,
        "client_secret": client_secret,
        "grant_type": "client_credentials",
        "scope": "public"
    });

    let resp = http
        .post("https://osu.ppy.sh/oauth/token")
        .header("Accept", "application/json")
        .json(&payload)
        .timeout(std::time::Duration::from_secs(10))
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let access_token = json.get("access_token")?.as_str()?.to_string();
    let expires_in = json.get("expires_in").and_then(|v| v.as_u64()).unwrap_or(86400);
    let valid_duration = std::time::Duration::from_secs(expires_in.saturating_sub(60).max(60));

    **token_guard = Some((access_token.clone(), Instant::now() + valid_duration));
    Some(access_token)
}

async fn fetch_country_placement(
    http: &reqwest::Client,
    token: &str,
    country: &str,
    pp: i32,
) -> Option<i32> {
    let fetch_page = |page: i64| {
        let url = format!(
            "https://osu.ppy.sh/api/v2/rankings/osu/performance?country={}&page={}",
            country, page
        );
        let http = http.clone();
        let auth = format!("Bearer {}", token);
        async move {
            let resp = http
                .get(&url)
                .header("Authorization", auth)
                .timeout(std::time::Duration::from_secs(10))
                .send()
                .await
                .ok()?;
            if !resp.status().is_success() {
                return None;
            }
            resp.json::<serde_json::Value>().await.ok()
        }
    };

    let first = fetch_page(1).await?;
    let ranking = first.get("ranking")?.as_array()?;
    if ranking.is_empty() {
        return None;
    }
    let size = ranking.len() as i64;
    if size == 0 {
        return None;
    }
    let total = first.get("total")?.as_i64()?;
    let pp_f = pp as f64;

    let mut low = 1;
    let mut high = (total + size - 1) / size;
    let mut candidate: Option<Vec<serde_json::Value>> = None;

    for _ in 0..16 {
        if low > high {
            break;
        }
        let mid = (low + high) / 2;
        let data = if mid == 1 {
            Some(first.clone())
        } else {
            fetch_page(mid).await
        };
        let entries = match data.as_ref().and_then(|d| d.get("ranking")).and_then(|r| r.as_array()) {
            Some(e) if !e.is_empty() => e,
            _ => return None,
        };

        let last_pp = entries.last().and_then(|e| e.get("pp")).and_then(|v| v.as_f64()).unwrap_or(0.0);
        if last_pp <= pp_f {
            candidate = Some(entries.clone());
            high = mid - 1;
        } else {
            low = mid + 1;
        }
    }

    if let Some(entries) = candidate {
        for entry in entries {
            let entry_pp = entry.get("pp").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if entry_pp <= pp_f {
                return entry.get("country_rank").and_then(|v| v.as_i64()).map(|r| r as i32);
            }
        }
    }

    None
}

pub async fn get_country_rank(
    http: &reqwest::Client,
    country: &str,
    pp: i32,
    creds: Option<(u64, &str)>,
) -> Option<i32> {
    if country.is_empty() || pp <= 0 {
        return None;
    }

    let (client_id, client_secret) = if let Some((id, sec)) = creds {
        (id, sec.to_string())
    } else {
        let client_id_str = std::env::var("OSU_CLIENT_ID").ok()?;
        let client_id: u64 = client_id_str.trim().parse().ok()?;
        let client_secret = std::env::var("OSU_CLIENT_SECRET").ok()?;
        (client_id, client_secret)
    };
    if client_secret.trim().is_empty() {
        return None;
    }

    let country_upper = country.to_uppercase();
    let cache_key = (country_upper.clone(), pp);

    {
        let mut guard = COUNTRY_RANK_CACHE.lock().await;
        let cache = guard.get_or_insert_with(HashMap::new);
        if let Some((cached_time, rank)) = cache.get(&cache_key) {
            if cached_time.elapsed().as_secs() < 900 {
                return *rank;
            }
        }
    }

    // Get or refresh OAuth token
    let token = {
        let mut token_guard = OAUTH_TOKEN.lock().await;
        let now = Instant::now();
        if let Some((tok, expires_at)) = &*token_guard {
            if *expires_at > now {
                tok.clone()
            } else {
                fetch_oauth_token(http, client_id, &client_secret, &mut token_guard).await?
            }
        } else {
            fetch_oauth_token(http, client_id, &client_secret, &mut token_guard).await?
        }
    };

    // Binary search country ranking
    let rank = fetch_country_placement(http, &token, &country_upper, pp).await;

    {
        let mut guard = COUNTRY_RANK_CACHE.lock().await;
        let cache = guard.get_or_insert_with(HashMap::new);
        cache.insert(cache_key, (Instant::now(), rank));
    }

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
        has_replay: s.has_replay,
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
        (&Method::GET, "avatar") => handle_avatar(state, params).await,
        (&Method::GET, "medals") => handle_medals_catalog().await,
        (&Method::POST, "login") => handle_login(state, headers, body).await,
        (&Method::POST, "logout") => handle_logout(state, headers).await,
        (&Method::POST, "settings") => handle_settings(state, headers, body).await,
        (&Method::POST, "scores/delete") => handle_delete_score(state, headers, body).await,
        (&Method::GET, "scores/replay") => handle_score_replay(state, params).await,
        (&Method::POST, "scores/pin") => handle_pin_score(state, headers, body).await,
        (&Method::POST, "import_official") => handle_import_official(state, headers, body).await,
        _ => json_error(StatusCode::NOT_FOUND, "Not found"),
    }
}

async fn handle_medals_catalog() -> Response {
    json_ok(crate::types::medal::get_all_medals())
        .with_header("Cache-Control", "public, max-age=86400")
}

async fn handle_profile(state: SharedState, params: &HashMap<String, String>) -> Response {
    let name = match params.get("name") {
        Some(n) if !n.trim().is_empty() => n.trim(),
        _ => return json_error(StatusCode::BAD_REQUEST, "Profile username is required."),
    };

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

    let mode_str = params.get("mode").map(|s| s.as_str()).unwrap_or("osu");
    if !["osu", "taiko", "fruits", "mania", "vn", "rx", "ap"].contains(&mode_str) {
        return json_error(StatusCode::BAD_REQUEST, "Unknown mode.");
    }

    let is_active = state_guard.player.as_ref().map_or(false, |p| p.name == name);
    let http_client = state_guard.http.clone();
    let daily_key = state_guard.config.osu_daily_api_key.clone();
    let oauth_creds = match (
        &state_guard.config.osu_client_id,
        &state_guard.config.osu_client_secret,
    ) {
        (Some(cid), Some(sec)) => cid.trim().parse::<u64>().ok().map(|id| (id, sec.clone())),
        _ => None,
    };

    drop(conn);
    drop(state_guard);

    // Filter scores by mode:
    let (rx_bit, ap_bit) = (
        Mods::RELAX.bits() as u32,
        Mods::AUTOPILOT.bits() as u32,
    );

    let target_ruleset: Option<i32> = match mode_str {
        "taiko" | "1" => Some(1),
        "fruits" | "catch" | "2" => Some(2),
        "mania" | "3" => Some(3),
        "osu" | "0" | "vn" | "rx" | "ap" => Some(0),
        _ => None,
    };

    let mode_scores: Vec<&db::ScoreWithBeatmap> = scores
        .iter()
        .filter(|s| {
            if let Some(ruleset) = target_ruleset {
                if s.mode != ruleset {
                    return false;
                }
            }
            match mode_str {
                "rx" => (s.mods & rx_bit) != 0,
                "ap" => (s.mods & ap_bit) != 0,
                _ => (s.mods & (rx_bit | ap_bit)) == 0,
            }
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

    // Pinned plays
    let pinned_public: Vec<PublicScore> = {
        let mut list = Vec::new();
        for pid in &details.pinned_scores {
            if let Some(s) = mode_scores.iter().find(|s| s.id.to_string() == *pid) {
                list.push(to_public_score(s));
            }
        }
        list
    };

    let now_ts = now_secs() as i64;
    let recent_24h_public: Vec<PublicScore> = mode_scores
        .iter()
        .filter(|s| s.time >= now_ts - 86400)
        .take(100)
        .map(|s| to_public_score(s))
        .collect();

    let last_play = mode_scores.first().map(|s| s.time);
    let first_play = scores.iter().map(|s| s.time).filter(|&t| t > 0).min();

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
        "taiko" => 1,
        "fruits" => 2,
        "mania" => 3,
        _ => 0,
    };

    let global_rank = get_daily_rank(&http_client, daily_key.as_deref(), calculated_pp).await;
    let country_rank = if (mode_str == "vn" || mode_str == "osu") && !details.country.is_empty() && calculated_pp > 0 {
        get_country_rank(
            &http_client,
            &details.country,
            calculated_pp,
            oauth_creds.as_ref().map(|(id, sec)| (*id, sec.as_str())),
        )
        .await
    } else {
        None
    };

    // Save rank snapshot if valid rank obtained
    let state_guard = state.read().await;
    let conn = state_guard.db.lock().await;
    if let Some(rank) = global_rank {
        let _ = db::record_rank_snapshot(&conn, name, mode_id, rank);
    }
    let rank_history = db::get_rank_history(&conn, name, mode_id, 90).unwrap_or_default();
    let mut rank_highest = db::get_highest_rank(&conn, name, mode_id).unwrap_or(None);
    if let Some(current_rank) = global_rank {
        let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
        if let Some(highest) = &mut rank_highest {
            if current_rank < highest.rank {
                highest.rank = current_rank;
                highest.updated_at = today;
            }
        } else {
            rank_highest = Some(crate::db::RankHighest {
                rank: current_rank,
                updated_at: today,
            });
        }
    }
    let user_medals = db::get_user_medals(&conn, name).unwrap_or_default();
    let medals_display: Vec<crate::types::medal::UserMedalDisplay> = user_medals
        .into_iter()
        .filter_map(|rec| {
            crate::types::medal::get_medal_by_id(rec.medal_id).map(|def| crate::types::medal::UserMedalDisplay {
                id: def.id,
                name: def.name.clone(),
                description: def.description.clone(),
                category: def.category.as_str().to_string(),
                icon_url: def.icon_url.clone(),
                achieved_at: rec.achieved_at,
            })
        })
        .collect();
    let av_url = db::get_avatar(&conn, name).unwrap_or(None);
    let avatar_version = Some(compute_avatar_version(av_url.as_deref()));
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
        country_rank,
        global_rank,
        rank_history,
        top: top_public,
        recent: recent_public,
        pinned: pinned_public,
        last_play,
        first_play,
        performance_history: perf_history,
        play_history,
        most_played,
        recent_24h: recent_24h_public,
        medals: medals_display,
        rank_highest,
        avatar_version,
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
    let (profiles, default, avatar_version) = {
        let conn = state_guard.db.lock().await;
        let p_list = db::list_profiles(&conn).unwrap_or_default();
        let def = db::get_default_profile(&conn).unwrap_or_default();
        let av_ver = username.as_ref().map(|u| {
            let av = db::get_avatar(&conn, u).unwrap_or(None);
            compute_avatar_version(av.as_deref())
        });
        (p_list, def, av_ver)
    };

    drop(state_guard);

    let mut resp = json_ok(&SessionResponse {
        csrf: csrf_token,
        user: username,
        profiles,
        active,
        default,
        avatar_version,
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

async fn serve_guest_avatar(state: &SharedState) -> Response {
    if let Some(asset) = EmbeddedAssets::get("vendor/avatar-guest@2x.01495bc4.png") {
        return Response::image(asset.data.to_vec(), "png")
            .with_header("Cache-Control", "no-cache, must-revalidate")
            .with_header("Pragma", "no-cache");
    }

    let state_read = state.read().await;
    if !state_read.default_avatar.is_empty() {
        return Response::image(state_read.default_avatar.clone(), "png")
            .with_header("Cache-Control", "no-cache, must-revalidate")
            .with_header("Pragma", "no-cache");
    }

    Response::not_found()
}

async fn handle_avatar(state: SharedState, params: &HashMap<String, String>) -> Response {
    let name = match params.get("name") {
        Some(n) if !n.trim().is_empty() => n.trim(),
        _ => return serve_guest_avatar(&state).await,
    };

    let state_read = state.read().await;
    let conn = state_read.db.lock().await;
    let avatar_url = db::get_avatar(&conn, name).unwrap_or(None);
    drop(conn);
    drop(state_read);

    let Some(pfp) = avatar_url else {
        return serve_guest_avatar(&state).await;
    };

    if let Some(path) = utils::is_path(&pfp) {
        if let Ok(bytes) = tokio::fs::read(&path).await {
            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
            return Response::image(bytes, ext)
                .with_header("Cache-Control", "no-cache, must-revalidate")
                .with_header("Pragma", "no-cache");
        }
    } else if pfp.starts_with("http://") || pfp.starts_with("https://") {
        let state_read = state.read().await;
        let client = state_read.http.clone();
        drop(state_read);

        if let Ok(resp) = client.get(&pfp).send().await {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    let ext = if pfp.ends_with(".jpg") || pfp.ends_with(".jpeg") {
                        "jpg"
                    } else if pfp.ends_with(".gif") {
                        "gif"
                    } else if pfp.ends_with(".webp") {
                        "webp"
                    } else {
                        "png"
                    };
                    return Response::image(bytes.to_vec(), ext)
                        .with_header("Cache-Control", "no-cache, must-revalidate")
                        .with_header("Pragma", "no-cache");
                }
            }
        }
    }

    serve_guest_avatar(&state).await
}

async fn handle_settings(
    state: SharedState,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let token = match cookie_val {
        Some(t) => t.to_string(),
        None => return json_error(StatusCode::UNAUTHORIZED, "Sign in to update your profile."),
    };

    let state_read = state.read().await;
    let session = match state_read.web_sessions.get(&token) {
        Some(s) if s.expires > now => s.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Session expired. Please sign in again."),
    };
    drop(state_read);

    if !can_write(Some(&session), headers) {
        return json_error(StatusCode::FORBIDDEN, "Refresh the page and try again.");
    }

    let current_user = match session.username {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Sign in to update your profile."),
    };

    let payload: SettingsPayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Invalid settings JSON: {}", e)),
    };

    // Validate details if present
    if let Some(ref details) = payload.details {
        if let Err(err_msg) = details.validate() {
            return json_error(StatusCode::BAD_REQUEST, err_msg);
        }
    }

    // Handle username rename if requested and different
    let target_username = if let Some(ref new_name_raw) = payload.username {
        let trimmed = new_name_raw.trim();
        if trimmed != current_user {
            let valid_name = match validate_username(trimmed) {
                Ok(n) => n,
                Err(e) => return json_error(StatusCode::BAD_REQUEST, e),
            };

            let s_read = state.read().await;
            let conn = s_read.db.lock().await;
            let exists = db::profile_exists(&conn, &valid_name).unwrap_or(false);
            if exists {
                return json_error(
                    StatusCode::BAD_REQUEST,
                    &format!("The username '{}' is already taken.", valid_name),
                );
            }
            if let Err(e) = db::rename_profile(&conn, &current_user, &valid_name) {
                return json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Failed to rename profile: {}", e),
                );
            }
            drop(conn);
            drop(s_read);
            valid_name
        } else {
            current_user.clone()
        }
    } else {
        current_user.clone()
    };

    // Handle avatar update or reset
    let mut old_avatar_to_delete: Option<PathBuf> = None;
    if payload.reset_avatar {
        let s_read = state.read().await;
        let conn = s_read.db.lock().await;
        if let Ok(Some(old_url)) = db::get_avatar(&conn, &target_username) {
            if let Some(p) = utils::is_path(&old_url) {
                old_avatar_to_delete = Some(p);
            }
        }
        let _ = db::clear_avatar(&conn, &target_username);
        drop(conn);
        drop(s_read);
    } else if let Some(ref avatar_data_str) = payload.avatar {
        let (ext, img_bytes) = match parse_and_validate_avatar(avatar_data_str) {
            Ok(res) => res,
            Err(err_msg) => return json_error(StatusCode::BAD_REQUEST, err_msg),
        };

        let avatar_dir = Path::new(".data").join("avatars");
        let _ = tokio::fs::create_dir_all(&avatar_dir).await;
        let token_hex = generate_hex_token(16);
        let filename = format!("{}.{}", token_hex, ext);
        let file_path = avatar_dir.join(&filename);

        if let Err(e) = tokio::fs::write(&file_path, &img_bytes).await {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to save avatar image: {}", e),
            );
        }

        let s_read = state.read().await;
        let conn = s_read.db.lock().await;
        if let Ok(Some(old_url)) = db::get_avatar(&conn, &target_username) {
            if let Some(p) = utils::is_path(&old_url) {
                old_avatar_to_delete = Some(p);
            }
        }
        let stored_path = file_path.to_string_lossy().to_string();
        let _ = db::set_avatar(&conn, &target_username, &stored_path);
        drop(conn);
        drop(s_read);
    }

    if let Some(old_path) = old_avatar_to_delete {
        let p_str = old_path.to_string_lossy();
        if p_str.contains(".data") && p_str.contains("avatars") {
            let _ = tokio::fs::remove_file(old_path).await;
        }
    }

    // Save profile details
    let mut country_byte: u8 = 0;
    if let Some(mut details) = payload.details {
        details.country = details.country.trim().to_uppercase();
        if !details.country.is_empty() {
            country_byte = crate::utils::country_code_to_byte(&details.country);
        }
        let s_read = state.read().await;
        let conn = s_read.db.lock().await;
        if let Err(e) = db::save_profile_details(&conn, &target_username, &details) {
            return json_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                &format!("Failed to save profile details: {}", e),
            );
        }
        drop(conn);
        drop(s_read);
    }

    // Synchronize in-memory AppState, sessions, and live in-game Player
    let mut s_write = state.write().await;
    if target_username != current_user {
        for s in s_write.web_sessions.values_mut() {
            if s.username.as_deref() == Some(&current_user) {
                s.username = Some(target_username.clone());
            }
        }
        if s_write.pending_login_name.as_deref() == Some(&current_user) {
            s_write.pending_login_name = Some(target_username.clone());
        }
        if s_write.config.osu_username.as_deref() == Some(&current_user) {
            s_write.config.osu_username = Some(target_username.clone());
        }
    }

    if let Some(ref mut p) = s_write.player {
        if p.name == current_user || p.name == target_username {
            if target_username != current_user {
                p.name = target_username.clone();
            }
            if country_byte > 0 {
                p.country = country_byte;
            }
            p.queue.extend_from_slice(&crate::packets::user_presence(p));
            p.queue.extend_from_slice(&crate::packets::user_stats(p));
        }
    }
    drop(s_write);

    json_ok(&LoginResponse { name: target_username })
}

async fn handle_delete_score(
    state: SharedState,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let token = match cookie_val {
        Some(t) => t.to_string(),
        None => return json_error(StatusCode::UNAUTHORIZED, "Sign in to manage your scores."),
    };

    let state_read = state.read().await;
    let session = match state_read.web_sessions.get(&token) {
        Some(s) if s.expires > now => s.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Session expired. Please sign in again."),
    };
    drop(state_read);

    if !can_write(Some(&session), headers) {
        return json_error(StatusCode::FORBIDDEN, "Refresh the page and try again.");
    }

    let current_user = match session.username {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Sign in to manage your scores."),
    };

    let payload: DeleteScorePayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Invalid request body: {}", e)),
    };

    {
        let state_guard = state.read().await;
        let conn = state_guard.db.lock().await;
        let deleted = match db::delete_score_by_id(&conn, &current_user, payload.id) {
            Ok(d) => d,
            Err(e) => {
                return json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Database error deleting score: {}", e),
                );
            }
        };

        if !deleted {
            return json_error(
                StatusCode::NOT_FOUND,
                "Score not found or not owned by your current profile.",
            );
        }

        let _ = db::decrement_playcount(&conn, &current_user);

        // Remove from pinned_scores if present
        if let Ok(mut details) = db::get_profile_details(&conn, &current_user) {
            let pid_str = payload.id.to_string();
            if details.pinned_scores.contains(&pid_str) {
                details.pinned_scores.retain(|x| x != &pid_str);
                let _ = db::save_profile_details(&conn, &current_user, &details);
            }
        }
    }

    // Recalculate profile PP, accuracy, total score, and osudaily rank
    let (_recalc_count, new_pp, new_acc) =
        crate::handlers::banchobot::recalculate_profile(&state, &current_user).await;

    // If player is currently online in-game, immediately broadcast updated stats
    let mut s_write = state.write().await;
    if let Some(ref mut p) = s_write.player {
        if p.name == current_user {
            p.pp = new_pp;
            p.acc = new_acc;
            p.queue.extend_from_slice(&crate::packets::user_stats(p));
            p.queue.extend_from_slice(&crate::packets::user_presence(p));
        }
    }
    drop(s_write);

    json_ok(&SuccessResponse { ok: true })
}

async fn handle_score_replay(state: SharedState, params: &HashMap<String, String>) -> Response {
    let id_str = match params.get("id") {
        Some(s) if !s.is_empty() => s,
        _ => return json_error(StatusCode::BAD_REQUEST, "Score ID is required."),
    };
    let score_id: i64 = match id_str.parse() {
        Ok(id) => id,
        Err(_) => return json_error(StatusCode::BAD_REQUEST, "Invalid score ID."),
    };

    let state_read = state.read().await;
    let conn = state_read.db.lock().await;
    let score = match db::get_score_by_id(&conn, score_id) {
        Ok(Some(s)) => s,
        _ => return json_error(StatusCode::NOT_FOUND, "Score not found."),
    };

    let osr_bytes = match score.to_osr_bytes_with_id(score_id) {
        Some(b) => b,
        None => return json_error(StatusCode::NOT_FOUND, "Replay data not available for this score."),
    };

    let beatmap = db::get_beatmap_by_md5(&conn, &score.md5).unwrap_or(None);
    drop(conn);
    drop(state_read);

    let (artist, title, version) = match beatmap {
        Some(b) => (b.artist, b.title, b.version),
        None => ("Unknown".to_string(), "Beatmap".to_string(), "".to_string()),
    };

    let safe_player: String = score.name.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == '[' || *c == ']' || *c == '-').collect();
    let safe_artist: String = artist.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == ' ' || *c == '-').collect();
    let safe_title: String = title.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == ' ' || *c == '-').collect();
    let safe_version: String = version.chars().filter(|c| c.is_alphanumeric() || *c == '_' || *c == ' ' || *c == '-').collect();

    let mods_str = score.mods_str.unwrap_or_else(|| {
        Mods::from_bits_truncate(score.mods).short_name()
    });

    let filename = format!("{safe_player} - {safe_artist} - {safe_title} [{safe_version}] ({mods_str}).osr");

    Response::new(osr_bytes)
        .with_header("Content-Type", "application/x-osu-replay")
        .with_header("Content-Disposition", &format!("attachment; filename=\"{filename}\""))
        .with_header("Cache-Control", "public, max-age=86400")
}

#[derive(Deserialize)]
struct PinScorePayload {
    id: String,
}

#[derive(Serialize, Deserialize)]
struct PinScoreResponse {
    pinned: bool,
    pinned_scores: Vec<String>,
}

async fn handle_pin_score(
    state: SharedState,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let token = match cookie_val {
        Some(t) => t.to_string(),
        None => return json_error(StatusCode::UNAUTHORIZED, "Sign in to manage pinned scores."),
    };

    let state_read = state.read().await;
    let session = match state_read.web_sessions.get(&token) {
        Some(s) if s.expires > now => s.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Session expired. Please sign in again."),
    };
    drop(state_read);

    if !can_write(Some(&session), headers) {
        return json_error(StatusCode::FORBIDDEN, "Refresh the page and try again.");
    }

    let current_user = match session.username {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Sign in to manage pinned scores."),
    };

    let payload: PinScorePayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Invalid request: {}", e)),
    };

    let s_read = state.read().await;
    let conn = s_read.db.lock().await;
    let mut details = db::get_profile_details(&conn, &current_user).unwrap_or_default();

    let is_pinned = if let Some(idx) = details.pinned_scores.iter().position(|id| id == &payload.id) {
        details.pinned_scores.remove(idx);
        false
    } else {
        if details.pinned_scores.len() >= 20 {
            return json_error(StatusCode::BAD_REQUEST, "You can pin at most 20 scores.");
        }
        details.pinned_scores.push(payload.id);
        true
    };

    if let Err(e) = db::save_profile_details(&conn, &current_user, &details) {
        return json_error(StatusCode::INTERNAL_SERVER_ERROR, &format!("Failed to update pinned scores: {}", e));
    }

    json_ok(&PinScoreResponse {
        pinned: is_pinned,
        pinned_scores: details.pinned_scores,
    })
}

async fn handle_import_official(
    state: SharedState,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    let now = now_secs();
    let cookie_val = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .and_then(|c| extract_cookie(c, SESSION_COOKIE_NAME));

    let token = match cookie_val {
        Some(t) => t.to_string(),
        None => return json_error(StatusCode::UNAUTHORIZED, "Sign in to import profile details."),
    };

    let state_read = state.read().await;
    let session = match state_read.web_sessions.get(&token) {
        Some(s) if s.expires > now => s.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Sign in to import profile details."),
    };
    drop(state_read);

    if !can_write(Some(&session), headers) {
        return json_error(StatusCode::FORBIDDEN, "Refresh the page and try again.");
    }

    let current_user = match session.username {
        Some(ref u) if !u.is_empty() => u.clone(),
        _ => return json_error(StatusCode::UNAUTHORIZED, "Sign in to import profile details."),
    };

    let payload: ImportOfficialPayload = match serde_json::from_slice(body) {
        Ok(p) => p,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &format!("Invalid request body: {}", e)),
    };

    let query = payload.query.trim();
    if query.is_empty() {
        return json_error(StatusCode::BAD_REQUEST, "Please provide an official osu! username or user ID.");
    }

    let encoded_query = percent_encoding::utf8_percent_encode(query, percent_encoding::NON_ALPHANUMERIC).to_string();
    let url = format!("https://osu.ppy.sh/users/{}", encoded_query);

    let http_client = {
        let s = state.read().await;
        s.http.clone()
    };

    let res = match http_client
        .get(&url)
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .timeout(std::time::Duration::from_secs(12))
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => {
            return json_error(StatusCode::BAD_GATEWAY, &format!("Failed to reach osu.ppy.sh: {}", e));
        }
    };

    if res.status() == StatusCode::NOT_FOUND {
        return json_error(StatusCode::NOT_FOUND, &format!("User '{}' not found on osu.ppy.sh.", query));
    }

    if !res.status().is_success() {
        return json_error(
            StatusCode::BAD_GATEWAY,
            &format!("osu.ppy.sh returned status {}.", res.status()),
        );
    }

    let html = match res.text().await {
        Ok(t) => t,
        Err(e) => {
            return json_error(
                StatusCode::BAD_GATEWAY,
                &format!("Failed to read response from osu.ppy.sh: {}", e),
            );
        }
    };

    let parsed = match parse_official_userpage_html(&html) {
        Ok(p) => p,
        Err(err) => return json_error(StatusCode::UNPROCESSABLE_ENTITY, &err),
    };

    let mut current_details = {
        let s = state.read().await;
        let conn = s.db.lock().await;
        db::get_profile_details(&conn, &current_user).unwrap_or_default()
    };

    let mut details_updated = false;
    if payload.import_bio {
        if let Some(bio) = parsed.raw_bio {
            current_details.about = safe_truncate(&bio, 5000).to_string();
            details_updated = true;
        }
    }

    if payload.import_details {
        if let Some(ref loc) = parsed.location {
            current_details.location = safe_truncate(loc, 100).to_string();
            details_updated = true;
        }
        if let Some(ref cc) = parsed.country_code {
            if crate::utils::country_code_to_byte(cc) != 0 {
                current_details.country = cc.clone();
                details_updated = true;
            }
        }
        if !parsed.playstyle.is_empty() {
            current_details.devices = parsed.playstyle.clone();
            details_updated = true;
        }
    }

    let mut avatar_data_url: Option<String> = None;
    let mut avatar_bytes_opt: Option<(Vec<u8>, &'static str)> = None;

    if payload.import_avatar {
        if let Some(ref av_url) = parsed.avatar_url {
            let final_av_url = if av_url.starts_with("//") {
                format!("https:{}", av_url)
            } else if av_url.starts_with('/') {
                format!("https://osu.ppy.sh{}", av_url)
            } else {
                av_url.clone()
            };

            if !final_av_url.contains("avatar-guest") {
                if let Ok(av_res) = http_client
                    .get(&final_av_url)
                    .timeout(std::time::Duration::from_secs(10))
                    .send()
                    .await
                {
                    if av_res.status().is_success() {
                        if let Ok(bytes) = av_res.bytes().await {
                            if let Ok(ext) = validate_image_format_and_dimensions(&bytes) {
                                let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
                                let data_url = format!("data:image/{};base64,{}", ext, b64);
                                avatar_data_url = Some(data_url);
                                avatar_bytes_opt = Some((bytes.to_vec(), ext));
                            }
                        }
                    }
                }
            }
        }
    }

    let mut avatar_updated = false;
    if payload.apply {
        let s_read = state.read().await;
        let conn = s_read.db.lock().await;

        if details_updated {
            if let Err(e) = db::save_profile_details(&conn, &current_user, &current_details) {
                return json_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Failed to save profile details: {}", e),
                );
            }
        }

        let mut old_avatar_to_delete: Option<PathBuf> = None;
        if let Some((img_bytes, ext)) = avatar_bytes_opt {
            let avatar_dir = Path::new(".data").join("avatars");
            let _ = tokio::fs::create_dir_all(&avatar_dir).await;
            let token_hex = generate_hex_token(16);
            let filename = format!("{}.{}", token_hex, ext);
            let file_path = avatar_dir.join(&filename);

            if let Ok(()) = tokio::fs::write(&file_path, &img_bytes).await {
                if let Ok(Some(old_url)) = db::get_avatar(&conn, &current_user) {
                    if let Some(p) = utils::is_path(&old_url) {
                        old_avatar_to_delete = Some(p);
                    }
                }
                let stored_path = file_path.to_string_lossy().to_string();
                let _ = db::set_avatar(&conn, &current_user, &stored_path);
                avatar_updated = true;
            }
        }

        let country_byte = if !current_details.country.is_empty() {
            crate::utils::country_code_to_byte(&current_details.country)
        } else {
            0
        };
        if country_byte != 0 {
            let _ = conn.execute(
                "UPDATE profiles SET country = ?1 WHERE name = ?2",
                rusqlite::params![country_byte, current_user],
            );
        }

        drop(conn);
        drop(s_read);

        if let Some(old_path) = old_avatar_to_delete {
            let p_str = old_path.to_string_lossy();
            if p_str.contains(".data") && p_str.contains("avatars") {
                let _ = tokio::fs::remove_file(old_path).await;
            }
        }

        let mut s_write = state.write().await;
        if let Some(ref mut player) = s_write.player {
            if player.name == current_user {
                if country_byte != 0 {
                    player.country = country_byte;
                }
                player.queue.extend_from_slice(&crate::packets::user_presence(player));
                player.queue.extend_from_slice(&crate::packets::user_stats(player));
            }
        }
        drop(s_write);
    }

    let message = if payload.apply {
        format!(
            "Successfully imported and applied official profile for '{}' (#{id})",
            parsed.username,
            id = parsed.id
        )
    } else {
        format!(
            "Successfully fetched official profile for '{}' (#{id})",
            parsed.username,
            id = parsed.id
        )
    };

    json_ok(&ImportOfficialResponse {
        ok: true,
        official_username: parsed.username,
        official_id: parsed.id,
        details: current_details,
        avatar_data_url,
        avatar_updated,
        details_updated,
        message,
    })
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
        crate::db::add_user_medal(&conn, "Alice", 1, 1700000000).unwrap();

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
        assert_eq!(data.medals.len(), 1);
        assert_eq!(data.medals[0].id, 1);
        assert_eq!(data.medals[0].name, "500 Combo");
        assert_eq!(data.medals[0].category, "Skill & Dedication");

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

        // Test osu mode (alias for standard mode 0)
        let mut params_osu = HashMap::new();
        params_osu.insert("name".to_string(), "Alice".to_string());
        params_osu.insert("mode".to_string(), "osu".to_string());
        let resp_osu = handle_profile(shared_state.clone(), &params_osu).await;
        assert_eq!(resp_osu.status, StatusCode::OK);
        let data_osu: ProfileResponse = serde_json::from_slice(&resp_osu.body).unwrap();
        assert_eq!(data_osu.mode, "osu");
        assert_eq!(data_osu.playcount, 2);

        // Test taiko mode (0 scores recorded for Alice in taiko)
        let mut params_taiko = HashMap::new();
        params_taiko.insert("name".to_string(), "Alice".to_string());
        params_taiko.insert("mode".to_string(), "taiko".to_string());
        let resp_taiko = handle_profile(shared_state.clone(), &params_taiko).await;
        assert_eq!(resp_taiko.status, StatusCode::OK);
        let data_taiko: ProfileResponse = serde_json::from_slice(&resp_taiko.body).unwrap();
        assert_eq!(data_taiko.mode, "taiko");
        assert_eq!(data_taiko.playcount, 0);
        assert_eq!(data_taiko.top.len(), 0);
    }

    #[test]
    fn test_image_validation_formats() {
        // Valid 1x1 PNG
        let valid_png = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, // magic
            0x00, 0x00, 0x00, 0x0D, // IHDR chunk length 13
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x00, 0x01, // width: 1
            0x00, 0x00, 0x00, 0x01, // height: 1
            0x08, 0x06, 0x00, 0x00, 0x00,
        ];
        assert_eq!(validate_image_format_and_dimensions(&valid_png).unwrap(), "png");

        // Oversized PNG (>4096)
        let oversized_png = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D,
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x10, 0x01, // width: 4097
            0x00, 0x00, 0x00, 0x01, // height: 1
            0x08, 0x06, 0x00, 0x00, 0x00,
        ];
        assert!(validate_image_format_and_dimensions(&oversized_png).is_err());

        // Valid GIF89a 100x200
        let mut valid_gif = vec![b'G', b'I', b'F', b'8', b'9', b'a'];
        valid_gif.extend_from_slice(&100u16.to_le_bytes()); // width
        valid_gif.extend_from_slice(&200u16.to_le_bytes()); // height
        valid_gif.push(0x80);
        assert_eq!(validate_image_format_and_dimensions(&valid_gif).unwrap(), "gif");

        // Corrupted header
        assert!(validate_image_format_and_dimensions(b"NOT_AN_IMAGE").is_err());
    }

    #[test]
    fn test_parse_and_validate_avatar_data_url() {
        let valid_png = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D,
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x00, 0x02,
            0x00, 0x00, 0x00, 0x02,
            0x08, 0x06, 0x00, 0x00, 0x00,
        ];
        let b64 = base64::engine::general_purpose::STANDARD.encode(&valid_png);
        let data_url = format!("data:image/png;base64,{}", b64);

        let (ext, bytes) = parse_and_validate_avatar(&data_url).unwrap();
        assert_eq!(ext, "png");
        assert_eq!(bytes, valid_png);

        // Invalid base64
        assert!(parse_and_validate_avatar("data:image/png;base64,???bad_base64???").is_err());
    }

    #[tokio::test]
    async fn test_avatar_route_fallback_and_custom() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "Bob").unwrap();

        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // Guest avatar when nonexistent or no custom avatar set
        let mut params = HashMap::new();
        params.insert("name".to_string(), "Bob".to_string());
        let resp = handle_avatar(shared_state.clone(), &params).await;
        assert_eq!(resp.status, StatusCode::OK);
        assert_eq!(resp.header("content-type").unwrap(), "image/png");
        assert_eq!(resp.header("cache-control").unwrap(), "no-cache, must-revalidate");
        assert_eq!(resp.header("pragma").unwrap(), "no-cache");

        // Custom avatar file path
        let temp_avatar = std::path::Path::new(".data").join("avatars").join("test_avatar.png");
        let _ = std::fs::create_dir_all(temp_avatar.parent().unwrap());
        std::fs::write(&temp_avatar, b"\x89PNG\r\n\x1a\ntest").unwrap();

        {
            let s = shared_state.read().await;
            let conn = s.db.lock().await;
            crate::db::set_avatar(&conn, "Bob", temp_avatar.to_str().unwrap()).unwrap();
        }

        let resp_custom = handle_avatar(shared_state.clone(), &params).await;
        assert_eq!(resp_custom.status, StatusCode::OK);
        assert_eq!(resp_custom.body, b"\x89PNG\r\n\x1a\ntest");
        assert_eq!(resp_custom.header("cache-control").unwrap(), "no-cache, must-revalidate");
        assert_eq!(resp_custom.header("pragma").unwrap(), "no-cache");

        let _ = std::fs::remove_file(&temp_avatar);
    }

    #[tokio::test]
    async fn test_settings_route_rename_details_and_avatar() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "OldName").unwrap();

        let mut app_state = AppState::new(conn, crate::types::config::Config::default());
        let mut player = crate::types::player::Player::new("OldName".to_string());
        player.country = 0;
        app_state.player = Some(player);

        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // 1. Establish session
        let headers_session = HeaderMap::new();
        let s_resp = handle_session(shared_state.clone(), &headers_session).await;
        let s_data: SessionResponse = serde_json::from_slice(&s_resp.body).unwrap();
        let set_cookie = s_resp.header("set-cookie").unwrap();
        let session_token = extract_cookie(set_cookie, SESSION_COOKIE_NAME).unwrap().to_string();

        // 2. Log in as OldName
        let mut login_headers = HeaderMap::new();
        login_headers.insert("cookie", HeaderValue::from_str(&format!("los_session={}", session_token)).unwrap());
        login_headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:5000"));
        login_headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        login_headers.insert("x-csrf-token", HeaderValue::from_str(&s_data.csrf).unwrap());
        let login_body = serde_json::json!({ "username": "OldName" }).to_string();
        let l_resp = handle_login(shared_state.clone(), &login_headers, login_body.as_bytes()).await;
        assert_eq!(l_resp.status, StatusCode::OK);

        // 3. Update settings: rename to NewName, set country to DE, set details, upload avatar
        let valid_png = [
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A,
            0x00, 0x00, 0x00, 0x0D,
            b'I', b'H', b'D', b'R',
            0x00, 0x00, 0x00, 0x10,
            0x00, 0x00, 0x00, 0x10,
            0x08, 0x06, 0x00, 0x00, 0x00,
        ];
        let avatar_b64 = format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&valid_png));

        let settings_body = serde_json::json!({
            "username": "NewName",
            "reset_avatar": false,
            "avatar": avatar_b64,
            "details": {
                "country": "DE",
                "location": "Berlin",
                "about": "New bio",
                "devices": ["Tablet", "Keyboard"],
                "section_order": ["me", "top-ranks", "historical", "beatmaps", "medals", "recent-activity"]
            }
        }).to_string();

        let settings_resp = handle_settings(shared_state.clone(), &login_headers, settings_body.as_bytes()).await;
        assert_eq!(settings_resp.status, StatusCode::OK);
        let ret: LoginResponse = serde_json::from_slice(&settings_resp.body).unwrap();
        assert_eq!(ret.name, "NewName");

        // Verify DB updates
        {
            let s = shared_state.read().await;
            let conn = s.db.lock().await;
            assert!(!crate::db::profile_exists(&conn, "OldName").unwrap());
            assert!(crate::db::profile_exists(&conn, "NewName").unwrap());
            assert_eq!(crate::db::get_profile_country(&conn, "NewName").unwrap(), 56); // DE = 56
            let det = crate::db::get_profile_details(&conn, "NewName").unwrap();
            assert_eq!(det.location, "Berlin");
            assert_eq!(det.about, "New bio");
            assert_eq!(det.country, "DE");

            // Verify avatar was saved
            let av = crate::db::get_avatar(&conn, "NewName").unwrap();
            assert!(av.is_some());
            let av_path = av.unwrap();
            assert!(std::path::Path::new(&av_path).exists());
            drop(conn);
            drop(s);

            let session_after = handle_session(shared_state.clone(), &login_headers).await;
            let s_after: SessionResponse = serde_json::from_slice(&session_after.body).unwrap();
            assert!(s_after.avatar_version.is_some());
            assert_ne!(s_after.avatar_version.as_deref(), Some("default"));

            let mut p_params = HashMap::new();
            p_params.insert("name".to_string(), "NewName".to_string());
            let profile_after = handle_profile(shared_state.clone(), &p_params).await;
            let p_data: ProfileResponse = serde_json::from_slice(&profile_after.body).unwrap();
            assert!(p_data.avatar_version.is_some());
            assert_ne!(p_data.avatar_version.as_deref(), Some("default"));

            let _ = std::fs::remove_file(&av_path);
        }

        // Verify live in-game player sync
        {
            let s = shared_state.read().await;
            let p = s.player.as_ref().unwrap();
            assert_eq!(p.name, "NewName");
            assert_eq!(p.country, 56);
            assert!(!p.queue.is_empty(), "Packets must be queued for in-game client");
        }
    }

    #[tokio::test]
    async fn test_delete_score_route_and_stats_recalculation() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "ScoreUser").unwrap();

        // Insert beatmap and score
        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.file_md5 = "map_del_1".to_string();
        bmap.title = "Delete Me".to_string();
        bmap.artist = "Artist".to_string();
        bmap.version = "Insane".to_string();
        bmap.beatmap_id = 9991;
        bmap.beatmapset_id = 9992;
        crate::db::insert_beatmap(&conn, &bmap).unwrap();

        let s1 = crate::types::score::Score {
            mode: 0,
            md5: "map_del_1".to_string(),
            name: "ScoreUser".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 1000000,
            max_combo: 500,
            perfect: true,
            mods: 0,
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(250.0),
            replay_md5: None,
            replay_frames: None,
            mods_str: None,
            scoreid: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let s1_id = crate::db::insert_score(&conn, &s1, "ranked").unwrap();
        crate::db::increment_playcount(&conn, "ScoreUser").unwrap();

        let mut app_state = AppState::new(conn, crate::types::config::Config::default());
        let player = crate::types::player::Player::new("ScoreUser".to_string());
        app_state.player = Some(player);

        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // 1. Establish session
        let s_resp = handle_session(shared_state.clone(), &HeaderMap::new()).await;
        let s_data: SessionResponse = serde_json::from_slice(&s_resp.body).unwrap();
        let set_cookie = s_resp.header("set-cookie").unwrap();
        let session_token = extract_cookie(set_cookie, SESSION_COOKIE_NAME).unwrap().to_string();

        // 2. Log in as ScoreUser
        let mut login_headers = HeaderMap::new();
        login_headers.insert("cookie", HeaderValue::from_str(&format!("los_session={}", session_token)).unwrap());
        login_headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:5000"));
        login_headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        login_headers.insert("x-csrf-token", HeaderValue::from_str(&s_data.csrf).unwrap());
        let login_body = serde_json::json!({ "username": "ScoreUser" }).to_string();
        let l_resp = handle_login(shared_state.clone(), &login_headers, login_body.as_bytes()).await;
        assert_eq!(l_resp.status, StatusCode::OK);

        // 3. Test deleting non-existent score -> 404
        let del_body_nf = serde_json::json!({ "id": 999999 }).to_string();
        let resp_nf = handle_delete_score(shared_state.clone(), &login_headers, del_body_nf.as_bytes()).await;
        assert_eq!(resp_nf.status, StatusCode::NOT_FOUND);

        // 4. Delete score with string ID "s1_id"
        let del_body = serde_json::json!({ "id": s1_id.to_string() }).to_string();
        let resp_del = handle_delete_score(shared_state.clone(), &login_headers, del_body.as_bytes()).await;
        assert_eq!(resp_del.status, StatusCode::OK);

        // Verify score is deleted from scores table and present in deleted_scores
        {
            let s = shared_state.read().await;
            let conn = s.db.lock().await;
            let remaining = crate::db::get_player_scores_with_beatmaps(&conn, "ScoreUser").unwrap();
            assert!(remaining.is_empty(), "Score should be deleted from scores table");

            let archived_count: i32 = conn.query_row(
                "SELECT COUNT(*) FROM deleted_scores WHERE player_name = 'ScoreUser' AND original_id = ?1",
                rusqlite::params![s1_id],
                |r| r.get(0),
            ).unwrap();
            assert_eq!(archived_count, 1, "Score should be archived in deleted_scores table");
        }

        // Verify live in-game player packet queue was updated
        {
            let s = shared_state.read().await;
            let p = s.player.as_ref().unwrap();
            assert!(!p.queue.is_empty(), "Packets must be queued for in-game stats update");
        }
    }

    #[test]
    fn test_decode_html_entities() {
        assert_eq!(decode_html_entities(""), "");
        assert_eq!(
            decode_html_entities("&quot;hello&quot; &amp; &lt;world&gt; &#039;test&#39; &apos;123&apos;"),
            "\"hello\" & <world> 'test' '123'"
        );
        assert_eq!(
            decode_html_entities("normal text without entities"),
            "normal text without entities"
        );
        assert_eq!(decode_html_entities("trailing & and &unknown; entity"), "trailing & and &unknown; entity");
    }

    #[test]
    fn test_safe_truncate() {
        let ascii = "hello world";
        assert_eq!(safe_truncate(ascii, 5), "hello");
        assert_eq!(safe_truncate(ascii, 20), "hello world");

        let unicode = "こんにちは世界"; // 7 chars, each 3 bytes = 21 bytes
        assert_eq!(safe_truncate(unicode, 6), "こん");
        assert_eq!(safe_truncate(unicode, 5), "こ"); // byte 5 is in middle of ん (bytes 3..6), truncates to 3
    }

    #[test]
    fn test_parse_official_userpage_html() {
        let mock_html = r#"
            <!DOCTYPE html>
            <html>
            <head><title>peppy</title></head>
            <body>
                <div class="js-react--profile-page" data-initial-data="{&quot;user&quot;:{&quot;id&quot;:2,&quot;username&quot;:&quot;peppy&quot;,&quot;country_code&quot;:&quot;AU&quot;,&quot;location&quot;:&quot;Melbourne&quot;,&quot;playstyle&quot;:[&quot;mouse&quot;,&quot;keyboard&quot;,&quot;touch&quot;],&quot;avatar_url&quot;:&quot;https://a.ppy.sh/2?1234&quot;,&quot;page&quot;:{&quot;raw&quot;:&quot;[b]Hi there[/b]&quot;}}}"></div>
            </body>
            </html>
        "#;

        let parsed = parse_official_userpage_html(mock_html).expect("Should parse valid profile HTML");
        assert_eq!(parsed.id, 2);
        assert_eq!(parsed.username, "peppy");
        assert_eq!(parsed.country_code.as_deref(), Some("AU"));
        assert_eq!(parsed.location.as_deref(), Some("Melbourne"));
        assert_eq!(parsed.playstyle, vec!["Mouse", "Keyboard", "Touchscreen"]);
        assert_eq!(parsed.raw_bio.as_deref(), Some("[b]Hi there[/b]"));
        assert_eq!(parsed.avatar_url.as_deref(), Some("https://a.ppy.sh/2?1234"));
    }

    #[test]
    fn test_parse_official_userpage_html_errors() {
        // Missing data-initial-data
        let html_no_data = "<html><body>No data here</body></html>";
        assert!(parse_official_userpage_html(html_no_data).is_err());

        // Malformed JSON
        let html_bad_json = "<div data-initial-data=\"not valid json\"></div>";
        assert!(parse_official_userpage_html(html_bad_json).is_err());

        // Missing username
        let html_no_user = "<div data-initial-data=\"{&quot;user&quot;:{&quot;id&quot;:1}}\"></div>";
        assert!(parse_official_userpage_html(html_no_user).is_err());
    }

    #[tokio::test]
    async fn test_import_official_route_session_and_csrf() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "ImportUser").unwrap();

        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // 1. Unauthenticated request -> 401
        let empty_headers = HeaderMap::new();
        let payload = serde_json::json!({ "query": "peppy" }).to_string();
        let resp_unauth = handle_import_official(shared_state.clone(), &empty_headers, payload.as_bytes()).await;
        assert_eq!(resp_unauth.status, StatusCode::UNAUTHORIZED);

        // 2. Establish session
        let s_resp = handle_session(shared_state.clone(), &HeaderMap::new()).await;
        let s_data: SessionResponse = serde_json::from_slice(&s_resp.body).unwrap();
        let set_cookie = s_resp.header("set-cookie").unwrap();
        let session_token = extract_cookie(set_cookie, SESSION_COOKIE_NAME).unwrap().to_string();

        let mut login_headers = HeaderMap::new();
        login_headers.insert("cookie", HeaderValue::from_str(&format!("los_session={}", session_token)).unwrap());
        login_headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:5000"));
        login_headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        login_headers.insert("x-csrf-token", HeaderValue::from_str(&s_data.csrf).unwrap());

        // 3. Log in
        let login_body = serde_json::json!({ "username": "ImportUser" }).to_string();
        let l_resp = handle_login(shared_state.clone(), &login_headers, login_body.as_bytes()).await;
        assert_eq!(l_resp.status, StatusCode::OK);

        // 4. Bad CSRF token -> 403
        let mut bad_csrf_headers = login_headers.clone();
        bad_csrf_headers.insert("x-csrf-token", HeaderValue::from_static("bad-token"));
        let resp_bad_csrf = handle_import_official(shared_state.clone(), &bad_csrf_headers, payload.as_bytes()).await;
        assert_eq!(resp_bad_csrf.status, StatusCode::FORBIDDEN);

        // 5. Empty query -> 400
        let empty_query_body = serde_json::json!({ "query": "   " }).to_string();
        let resp_bad_query = handle_import_official(shared_state.clone(), &login_headers, empty_query_body.as_bytes()).await;
        assert_eq!(resp_bad_query.status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_get_country_rank_empty_and_fallback() {
        let http = reqwest::Client::new();
        assert_eq!(get_country_rank(&http, "", 1000, None).await, None);
        assert_eq!(get_country_rank(&http, "DE", 0, None).await, None);
        assert_eq!(get_country_rank(&http, "DE", -50, None).await, None);
    }

    #[tokio::test]
    async fn test_scores_pin_and_replay_routes() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::init_db(&conn).unwrap();
        crate::db::ensure_profile(&conn, "ReplayUser").unwrap();

        let s1 = crate::types::Score {
            mode: 0,
            md5: "map_replay_md5".to_string(),
            name: "ReplayUser".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 50,
            nkatu: 0,
            nmiss: 0,
            score: 1000000,
            max_combo: 500,
            perfect: true,
            mods: 0,
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(300.0),
            replay_md5: Some("rep_md5_123".to_string()),
            replay_frames: Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b"test_frames_data")),
            mods_str: Some("NM".to_string()),
            scoreid: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let s1_id = crate::db::insert_score(&conn, &s1, "ranked").unwrap();

        let app_state = AppState::new(conn, crate::types::config::Config::default());
        let shared_state = std::sync::Arc::new(tokio::sync::RwLock::new(app_state));

        // 1. Download replay route
        let mut params = HashMap::new();
        params.insert("id".to_string(), s1_id.to_string());
        let replay_resp = handle_score_replay(shared_state.clone(), &params).await;
        assert_eq!(replay_resp.status, StatusCode::OK);
        assert_eq!(replay_resp.header("content-type"), Some("application/x-osu-replay"));
        assert!(replay_resp.body.len() > 20);

        // 2. Establish session for Pin/Unpin
        let s_resp = handle_session(shared_state.clone(), &HeaderMap::new()).await;
        let s_data: SessionResponse = serde_json::from_slice(&s_resp.body).unwrap();
        let set_cookie = s_resp.header("set-cookie").unwrap();
        let session_token = extract_cookie(set_cookie, SESSION_COOKIE_NAME).unwrap().to_string();

        let mut headers = HeaderMap::new();
        headers.insert("cookie", HeaderValue::from_str(&format!("los_session={}", session_token)).unwrap());
        headers.insert("origin", HeaderValue::from_static("http://127.0.0.1:5000"));
        headers.insert("host", HeaderValue::from_static("127.0.0.1:5000"));
        headers.insert("x-csrf-token", HeaderValue::from_str(&s_data.csrf).unwrap());

        // Log in
        let login_body = serde_json::json!({ "username": "ReplayUser" }).to_string();
        let l_resp = handle_login(shared_state.clone(), &headers, login_body.as_bytes()).await;
        assert_eq!(l_resp.status, StatusCode::OK);

        // Pin score
        let pin_payload = serde_json::json!({ "id": s1_id.to_string() }).to_string();
        let pin_resp = handle_pin_score(shared_state.clone(), &headers, pin_payload.as_bytes()).await;
        assert_eq!(pin_resp.status, StatusCode::OK);
        let pin_data: PinScoreResponse = serde_json::from_slice(&pin_resp.body).unwrap();
        assert!(pin_data.pinned);
        assert_eq!(pin_data.pinned_scores, vec![s1_id.to_string()]);

        // Unpin score
        let unpin_resp = handle_pin_score(shared_state.clone(), &headers, pin_payload.as_bytes()).await;
        assert_eq!(unpin_resp.status, StatusCode::OK);
        let unpin_data: PinScoreResponse = serde_json::from_slice(&unpin_resp.body).unwrap();
        assert!(!unpin_data.pinned);
        assert!(unpin_data.pinned_scores.is_empty());
    }
}

