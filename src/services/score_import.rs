use crate::types::beatmap::Beatmap;
use crate::types::config::Config;
use crate::types::mods::Mods;
use crate::types::replay::Replay;
use crate::types::score::Score;
use crate::utils;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum BanchoModItem {
    Str(String),
    Obj {
        acronym: String,
        #[serde(default)]
        settings: Option<serde_json::Value>,
    },
}

impl BanchoModItem {
    pub fn acronym(&self) -> &str {
        match self {
            BanchoModItem::Str(s) => s.as_str(),
            BanchoModItem::Obj { acronym, .. } => acronym.as_str(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanchoStatistics {
    #[serde(default)]
    pub count_50: i32,
    #[serde(default)]
    pub count_100: i32,
    #[serde(default)]
    pub count_300: i32,
    #[serde(default)]
    pub count_geki: Option<i32>,
    #[serde(default)]
    pub count_katu: Option<i32>,
    #[serde(default)]
    pub count_miss: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanchoBeatmapsetMeta {
    pub id: i64,
    pub title: String,
    pub artist: String,
    #[serde(default)]
    pub creator: String,
    #[serde(default)]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanchoBeatmapMeta {
    pub id: i64,
    pub beatmapset_id: i64,
    #[serde(default)]
    pub checksum: Option<String>,
    pub version: String,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub difficulty_rating: Option<f64>,
    #[serde(default)]
    pub total_length: Option<i32>,
    #[serde(default)]
    pub bpm: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanchoWebScore {
    pub id: i64,
    #[serde(default)]
    pub best_id: Option<i64>,
    #[serde(default)]
    pub user_id: i64,
    #[serde(default)]
    pub accuracy: f64,
    #[serde(default)]
    pub mods: Vec<BanchoModItem>,
    #[serde(default)]
    pub score: Option<i64>,
    #[serde(default)]
    pub legacy_score: Option<i64>,
    #[serde(default)]
    pub classic_total_score: Option<i64>,
    #[serde(default)]
    pub total_score: Option<i64>,
    #[serde(default)]
    pub max_combo: i32,
    #[serde(default)]
    pub passed: Option<bool>,
    #[serde(default)]
    pub perfect: Option<bool>,
    #[serde(default)]
    pub pp: Option<f64>,
    #[serde(default)]
    pub rank: Option<String>,
    pub created_at: String,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub mode_int: Option<i32>,
    #[serde(default)]
    pub replay: Option<bool>,
    pub statistics: BanchoStatistics,
    #[serde(default)]
    pub beatmap: Option<BanchoBeatmapMeta>,
    #[serde(default)]
    pub beatmapset: Option<BanchoBeatmapsetMeta>,
}

pub fn parse_mode_str(mode: &str) -> i32 {
    match mode.trim().to_lowercase().as_str() {
        "taiko" | "1" => 1,
        "fruits" | "catch" | "ctb" | "2" => 2,
        "mania" | "3" => 3,
        _ => 0,
    }
}

pub fn mode_to_ruleset_str(mode: i32) -> &'static str {
    match mode {
        1 => "taiko",
        2 => "fruits",
        3 => "mania",
        _ => "osu",
    }
}

pub fn parse_bancho_mods(mod_items: &[BanchoModItem]) -> (Mods, String) {
    let mut combined_str = String::new();
    for item in mod_items {
        let acr = item.acronym().trim().to_uppercase();
        // Skip informational/visual-only non-gameplay mods if needed, or append standard acronyms
        if acr != "CL" {
            combined_str.push_str(&acr);
        }
    }
    let mods = Mods::from_short_str(&combined_str);
    let str_repr = mods.short_name();
    (mods, str_repr)
}

pub async fn resolve_user_id_and_name(
    http: &reqwest::Client,
    query: &str,
) -> Result<(i64, String), String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("User query is empty".to_string());
    }

    if let Ok(id) = trimmed.parse::<i64>() {
        return Ok((id, id.to_string()));
    }

    let encoded = percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC).to_string();
    let url = format!("https://osu.ppy.sh/users/{}", encoded);

    let resp = http.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .header("Accept-Language", "en-US,en;q=0.9")
        .send().await
        .map_err(|e| format!("Failed to reach osu.ppy.sh: {}", e))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Err(format!("User '{}' not found on osu.ppy.sh", trimmed));
    }

    if !resp.status().is_success() {
        return Err(format!("osu.ppy.sh returned status {}", resp.status()));
    }

    let html = resp.text().await
        .map_err(|e| format!("Failed to read osu.ppy.sh response: {}", e))?;

    let parsed = crate::handlers::website::parse_official_userpage_html(&html)
        .map_err(|e| format!("Failed to parse user profile: {}", e))?;

    Ok((parsed.id, parsed.username))
}

pub async fn fetch_user_scores_web(
    http: &reqwest::Client,
    user_id: i64,
    score_type: &str,
    mode_str: &str,
    limit: usize,
    session_cookie: Option<&str>,
) -> Result<Vec<BanchoWebScore>, String> {
    let lim = limit.min(100);
    let url = format!(
        "https://osu.ppy.sh/users/{}/scores/{}?mode={}&limit={}&offset=0",
        user_id, score_type, mode_str, lim
    );

    let mut req = http.get(&url)
        .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
        .header("Accept", "application/json")
        .header("Referer", "https://osu.ppy.sh/");

    if let Some(cookie) = session_cookie {
        let cookie_str = if cookie.starts_with("osu_session=") {
            cookie.to_string()
        } else {
            format!("osu_session={}", cookie)
        };
        req = req.header("Cookie", cookie_str);
    }

    let resp = req.send().await
        .map_err(|e| format!("Failed to query osu! scores: {}", e))?;

    if resp.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(Vec::new());
    }

    if !resp.status().is_success() {
        return Err(format!("osu! scores endpoint returned HTTP {}", resp.status()));
    }

    let body = resp.text().await
        .map_err(|e| format!("Failed to read response body: {}", e))?;

    let scores: Vec<BanchoWebScore> = serde_json::from_str(&body)
        .map_err(|e| format!("Failed to deserialize Bancho scores JSON: {}", e))?;

    Ok(scores)
}

pub async fn fetch_missing_beatmap_osu(
    http: &reqwest::Client,
    config: &Config,
    bmap_id: i64,
    md5: &str,
) -> Option<Beatmap> {
    // 1. Try local Songs folder first
    if let Some(songs_dir) = utils::resolve_songs_folder(config) {
        let mid = if bmap_id > 0 { Some(bmap_id) } else { None };
        let md5_opt = if !md5.is_empty() { Some(md5) } else { None };
        if let Some((mut local_bmap, content)) = utils::find_and_parse_local_osu_file(&songs_dir, None, mid, md5_opt, None) {
            local_bmap.file_content = Some(content);
            return Some(local_bmap);
        }
    }

    // 2. Fetch raw .osu from official osu.ppy.sh/osu/{id}
    if bmap_id > 0 {
        if let Some(content) = utils::fetch_osu_file(http, bmap_id).await {
            if let Some(mut bmap) = utils::parse_osu_file_to_beatmap(&content, None, None) {
                if bmap.beatmap_id == 0 {
                    bmap.beatmap_id = bmap_id;
                }
                bmap.file_content = Some(content);
                return Some(bmap);
            }
        }

        // 3. Fallback to Catboy mirror
        let catboy_url = format!("https://catboy.best/osu/{}", bmap_id);
        if let Ok(resp) = http.get(&catboy_url).send().await {
            if resp.status().is_success() {
                if let Ok(content) = resp.text().await {
                    if let Some(mut bmap) = utils::parse_osu_file_to_beatmap(&content, None, None) {
                        if bmap.beatmap_id == 0 {
                            bmap.beatmap_id = bmap_id;
                        }
                        bmap.file_content = Some(content);
                        return Some(bmap);
                    }
                }
            }
        }
    }

    None
}

pub async fn download_score_replay(
    http: &reqwest::Client,
    ruleset: &str,
    score_id: i64,
    session_cookie: Option<&str>,
    api_key: Option<&str>,
    beatmap_id: i64,
    user_id: i64,
    mode: i32,
) -> Option<(String, Option<String>)> {
    // 1. Try direct .osr download via session cookie
    if let Some(cookie) = session_cookie {
        let url = format!("https://osu.ppy.sh/scores/{}/{}/download", ruleset, score_id);
        let cookie_val = if cookie.starts_with("osu_session=") {
            cookie.to_string()
        } else {
            format!("osu_session={}", cookie)
        };
        if let Ok(resp) = http.get(&url)
            .header("Cookie", cookie_val)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36")
            .header("Referer", "https://osu.ppy.sh/")
            .send().await
        {
            if resp.status().is_success() {
                if let Ok(bytes) = resp.bytes().await {
                    if let Ok(replay) = Replay::from_bytes(bytes.to_vec()) {
                        let b64 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &replay.raw_frames);
                        return Some((b64, Some(replay.replay_md5)));
                    }
                }
            }
        }
    }

    // 2. Try API v1 get_replay fallback if key is configured
    if let Some(key) = api_key {
        if beatmap_id > 0 && user_id > 0 {
            let url = format!(
                "https://osu.ppy.sh/api/get_replay?k={}&b={}&u={}&m={}",
                key, beatmap_id, user_id, mode
            );
            if let Ok(resp) = http.get(&url).send().await {
                if resp.status().is_success() {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(content) = json.get("content").and_then(|v| v.as_str()) {
                            if !content.is_empty() {
                                return Some((content.to_string(), None));
                            }
                        }
                    }
                }
            }
        }
    }

    None
}

pub fn parse_web_score_to_local(
    raw: &BanchoWebScore,
    local_player: &str,
    target_mode: i32,
    recalc_pp: bool,
    beatmap_content: Option<&str>,
) -> (Score, Option<Beatmap>) {
    let mode = raw.mode_int.unwrap_or_else(|| {
        raw.mode.as_deref().map(parse_mode_str).unwrap_or(target_mode)
    });

    let (mods, mods_str) = parse_bancho_mods(&raw.mods);
    let md5 = raw.beatmap.as_ref()
        .and_then(|b| b.checksum.clone())
        .unwrap_or_default();

    let score_val = raw.legacy_score
        .or(raw.classic_total_score)
        .or(raw.score)
        .or(raw.total_score)
        .unwrap_or(0);

    let n300 = raw.statistics.count_300;
    let n100 = raw.statistics.count_100;
    let n50 = raw.statistics.count_50;
    let ngeki = raw.statistics.count_geki.unwrap_or(0);
    let nkatu = raw.statistics.count_katu.unwrap_or(0);
    let nmiss = raw.statistics.count_miss;

    let acc_calculated = utils::calculate_accuracy(mode as u8, n300, n100, n50, ngeki, nkatu, nmiss);
    let acc_final = if acc_calculated > 0.0 {
        acc_calculated
    } else {
        (raw.accuracy * 10000.0).round() / 100.0
    };

    let time_parsed = chrono::DateTime::parse_from_rfc3339(&raw.created_at)
        .map(|dt| dt.timestamp())
        .unwrap_or_else(|_| chrono::Utc::now().timestamp());

    let bmap_status = raw.beatmap.as_ref()
        .and_then(|b| b.status.clone())
        .unwrap_or_else(|| "ranked".to_string());

    let mut pp_final = raw.pp;
    if (pp_final.is_none() || recalc_pp) && beatmap_content.is_some() {
        if let Some(content) = beatmap_content {
            if let Ok(parsed_map) = rosu_pp::Beatmap::from_bytes(content.as_bytes()) {
                let game_mode = match mode {
                    0 => rosu_pp::model::mode::GameMode::Osu,
                    1 => rosu_pp::model::mode::GameMode::Taiko,
                    2 => rosu_pp::model::mode::GameMode::Catch,
                    3 => rosu_pp::model::mode::GameMode::Mania,
                    _ => rosu_pp::model::mode::GameMode::Osu,
                };
                let result = rosu_pp::Performance::new(&parsed_map)
                    .mode_or_ignore(game_mode)
                    .mods(mods.bits())
                    .combo(raw.max_combo as u32)
                    .n300(n300 as u32)
                    .n100(n100 as u32)
                    .n50(n50 as u32)
                    .misses(nmiss as u32)
                    .calculate();
                pp_final = Some(result.pp());
            }
        }
    }

    let local_score = Score {
        mode,
        md5: md5.clone(),
        name: local_player.to_string(),
        n300,
        n100,
        n50,
        ngeki,
        nkatu,
        nmiss,
        score: score_val,
        max_combo: raw.max_combo,
        perfect: raw.perfect.unwrap_or(false),
        mods: mods.bits(),
        time: time_parsed,
        acc: Some(acc_final),
        pp: pp_final,
        replay_md5: None,
        replay_frames: None,
        mods_str: Some(mods_str),
        scoreid: None,
        additional_mods: None,
        submission_checksum: None,
        submission_identity: Some(format!("bancho:{}", raw.id)),
    };

    let bmap_opt = if let (Some(bmeta), Some(smeta)) = (&raw.beatmap, &raw.beatmapset) {
        let mut b = Beatmap::blank();
        b.beatmap_id = bmeta.id;
        b.beatmapset_id = smeta.id;
        b.file_md5 = md5;
        b.title = smeta.title.clone();
        b.artist = smeta.artist.clone();
        b.creator = smeta.creator.clone();
        b.version = bmeta.version.clone();
        b.mode = mode;
        b.bpm = bmeta.bpm.unwrap_or(0.0);
        b.total_length = bmeta.total_length.unwrap_or(0);
        b.difficultyrating = bmeta.difficulty_rating.unwrap_or(0.0);
        b.approved = match bmap_status.as_str() {
            "ranked" => 1,
            "approved" => 2,
            "qualified" => 3,
            "loved" => 4,
            _ => 0,
        };
        Some(b)
    } else {
        None
    };

    (local_score, bmap_opt)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bancho_score_json_parsing_with_lazer_mods() {
        let json_data = r#"[
            {
                "id": 4321987654,
                "best_id": 4321987654,
                "user_id": 124493,
                "accuracy": 0.9912,
                "mods": [
                    "HD",
                    {"acronym": "DT", "settings": {"speed_change": 1.5}},
                    {"acronym": "CL"}
                ],
                "score": 1000000,
                "legacy_score": 11524310,
                "max_combo": 1145,
                "passed": true,
                "perfect": true,
                "pp": 742.85,
                "rank": "SH",
                "created_at": "2024-03-15T18:22:31Z",
                "mode": "osu",
                "mode_int": 0,
                "statistics": {
                    "count_50": 0,
                    "count_100": 8,
                    "count_300": 782,
                    "count_geki": 190,
                    "count_katu": 8,
                    "count_miss": 0
                },
                "beatmap": {
                    "id": 1816113,
                    "beatmapset_id": 869207,
                    "checksum": "c33f81e6498ec5179daff3d790ff456e",
                    "version": "Extra",
                    "status": "ranked",
                    "difficulty_rating": 6.82,
                    "total_length": 182,
                    "bpm": 240.0
                },
                "beatmapset": {
                    "id": 869207,
                    "title": "Koko Soko",
                    "artist": "Smile.dk",
                    "creator": "Sotarks",
                    "status": "ranked"
                },
                "replay": true
            }
        ]"#;

        let parsed: Vec<BanchoWebScore> = serde_json::from_str(json_data).expect("Should parse lazer mods JSON");
        assert_eq!(parsed.len(), 1);
        let s = &parsed[0];
        assert_eq!(s.id, 4321987654);
        assert_eq!(s.legacy_score, Some(11524310));
        assert_eq!(s.mods.len(), 3);
        assert_eq!(s.mods[0].acronym(), "HD");
        assert_eq!(s.mods[1].acronym(), "DT");
        assert_eq!(s.mods[2].acronym(), "CL");

        let (mods_flag, mods_str) = parse_bancho_mods(&s.mods);
        assert!(mods_flag.contains(Mods::HIDDEN));
        assert!(mods_flag.contains(Mods::DOUBLETIME));
        assert_eq!(mods_str, "HDDT");
    }

    #[test]
    fn test_bancho_score_to_local_conversion() {
        let raw = BanchoWebScore {
            id: 999888,
            best_id: None,
            user_id: 1234,
            accuracy: 0.985,
            mods: vec![BanchoModItem::Str("HR".to_string())],
            score: Some(800000),
            legacy_score: Some(15000000),
            classic_total_score: None,
            total_score: None,
            max_combo: 850,
            passed: Some(true),
            perfect: Some(true),
            pp: Some(350.2),
            rank: Some("S".to_string()),
            created_at: "2023-01-01T00:00:00Z".to_string(),
            mode: Some("osu".to_string()),
            mode_int: Some(0),
            replay: Some(false),
            statistics: BanchoStatistics {
                count_50: 2,
                count_100: 10,
                count_300: 500,
                count_geki: None,
                count_katu: None,
                count_miss: 0,
            },
            beatmap: Some(BanchoBeatmapMeta {
                id: 5555,
                beatmapset_id: 4444,
                checksum: Some("test_md5_hash".to_string()),
                version: "Hard".to_string(),
                status: Some("ranked".to_string()),
                difficulty_rating: Some(4.5),
                total_length: Some(120),
                bpm: Some(180.0),
            }),
            beatmapset: Some(BanchoBeatmapsetMeta {
                id: 4444,
                title: "Song Title".to_string(),
                artist: "Song Artist".to_string(),
                creator: "Song Creator".to_string(),
                status: Some("ranked".to_string()),
            }),
        };

        let (score, bmap_opt) = parse_web_score_to_local(&raw, "LocalHero", 0, false, None);
        assert_eq!(score.name, "LocalHero");
        assert_eq!(score.score, 15000000); // Prefers legacy_score
        assert_eq!(score.submission_identity, Some("bancho:999888".to_string()));
        assert_eq!(score.pp, Some(350.2)); // Preserved official PP
        assert_eq!(score.mods_str.as_deref(), Some("HR"));

        let bmap = bmap_opt.expect("Beatmap should be extracted");
        assert_eq!(bmap.beatmap_id, 5555);
        assert_eq!(bmap.title, "Song Title");
        assert_eq!(bmap.file_md5, "test_md5_hash");
        assert_eq!(bmap.approved, 1);
    }

    #[test]
    fn test_mode_helpers() {
        assert_eq!(parse_mode_str("osu"), 0);
        assert_eq!(parse_mode_str("taiko"), 1);
        assert_eq!(parse_mode_str("fruits"), 2);
        assert_eq!(parse_mode_str("catch"), 2);
        assert_eq!(parse_mode_str("mania"), 3);
        assert_eq!(parse_mode_str("unknown"), 0);

        assert_eq!(mode_to_ruleset_str(0), "osu");
        assert_eq!(mode_to_ruleset_str(1), "taiko");
        assert_eq!(mode_to_ruleset_str(2), "fruits");
        assert_eq!(mode_to_ruleset_str(3), "mania");
    }
}
