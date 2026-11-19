use crate::db;
use crate::handlers::website::parse_official_userpage_html;
use crate::multiplayer::bot::{infer_target_sr_from_pp, BotProfile, BotSource, SkillProfile};
use crate::multiplayer::simulator::ConstrainedScore;
use crate::services::score_import::{mode_to_ruleset_str, parse_bancho_mods, BanchoModItem};
use crate::state::AppState;
use crate::utils;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

pub const PLAYER_CACHE_TTL: Duration = Duration::from_secs(30 * 60);
pub const MAP_SCORE_CACHE_TTL: Duration = Duration::from_secs(15 * 60);

pub type MimicMapScore = ConstrainedScore;

#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedPlayer {
    pub userid: i32,
    pub username: String,
    pub country: u8,
    pub rank: i32,
    pub pp: i32,
    pub acc: f64,
}

pub fn is_cache_fresh(inserted_at: Instant, ttl: Duration, now: Instant) -> bool {
    now.saturating_duration_since(inserted_at) < ttl
}

pub fn skill_from_resolved_player(p: &ResolvedPlayer) -> SkillProfile {
    let target_sr = infer_target_sr_from_pp(p.pp);
    let accuracy_mean = if p.acc > 0.0 {
        p.acc.clamp(75.0, 99.8)
    } else {
        97.5
    };
    let pp_norm = ((p.pp.max(0) as f64) / 20_000.0).clamp(0.0, 1.0);
    let consistency = (0.60 + 0.36 * pp_norm).clamp(0.55, 0.96);
    let choke_tendency = (0.26 - 0.20 * pp_norm).clamp(0.05, 0.30);

    SkillProfile {
        target_sr,
        accuracy_mean,
        consistency,
        choke_tendency,
        preferred_mods: 0,
        can_fail: true,
    }
}

pub fn bot_profile_from_resolved(p: ResolvedPlayer) -> BotProfile {
    let fallback_skill = skill_from_resolved_player(&p);
    BotProfile {
        userid: p.userid,
        name: p.username.clone(),
        country: p.country,
        rank: p.rank,
        pp: p.pp,
        acc: fallback_skill.accuracy_mean,
        source: BotSource::MimicPlayer {
            username: p.username,
            osu_user_id: Some(p.userid),
            fallback_skill,
        },
    }
}

/// Extracts `(pp, rank, acc)` from osu.ppy.sh's embedded `data-initial-data` HTML JSON when available.
fn extract_web_user_stats(html: &str) -> (i32, i32, f64) {
    let marker_double = "data-initial-data=\"";
    let marker_single = "data-initial-data='";
    let raw = if let Some(pos) = html.find(marker_double) {
        let start = pos + marker_double.len();
        html[start..].find('"').map(|end| &html[start..start + end])
    } else if let Some(pos) = html.find(marker_single) {
        let start = pos + marker_single.len();
        html[start..].find('\'').map(|end| &html[start..start + end])
    } else {
        None
    };

    let Some(escaped) = raw else {
        return (0, 0, 0.0);
    };
    let unescaped = escaped
        .replace("&quot;", "\"")
        .replace("&#34;", "\"")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">");

    let Ok(val) = serde_json::from_str::<serde_json::Value>(&unescaped) else {
        return (0, 0, 0.0);
    };
    let stats = val.pointer("/user/statistics").or_else(|| val.pointer("/statistics"));
    let Some(st) = stats else {
        return (0, 0, 0.0);
    };

    let pp = st.get("pp").and_then(|v| v.as_f64()).unwrap_or(0.0).round() as i32;
    let rank = st.get("global_rank").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let acc = st.get("hit_accuracy").and_then(|v| v.as_f64()).unwrap_or(0.0);
    (pp, rank, acc)
}

/// Resolves a real osu! player via local friends -> TTL cache -> osu.ppy.sh web / API.
/// All external HTTP calls are executed outside `AppState` locks.
pub async fn resolve_player(state: &Arc<RwLock<AppState>>, local_player_name: &str, query: &str) -> Result<BotProfile, String> {
    let trimmed = query.trim();
    if trimmed.is_empty() {
        return Err("Player name cannot be empty.".to_string());
    }
    let cache_key = trimmed.to_ascii_lowercase();

    // 1. Check local friends table first (instant offline resolution)
    let friend_match = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let friends = db::get_friend_records(&db_conn, local_player_name).unwrap_or_default();
        let query_id = trimmed.parse::<i32>().ok();
        friends.into_iter().find(|f| {
            f.friend_id > 4
                && (f.friend_name.eq_ignore_ascii_case(trimmed) || query_id.is_some_and(|qid| f.friend_id == qid))
        })
    };

    if let Some(f) = friend_match {
        let resolved = ResolvedPlayer {
            userid: f.friend_id,
            username: if f.friend_name.is_empty() { trimmed.to_string() } else { f.friend_name },
            country: f.country,
            rank: f.rank,
            pp: f.pp,
            acc: f.acc,
        };
        return Ok(bot_profile_from_resolved(resolved));
    }

    // 2. Check TTL player cache
    let now = Instant::now();
    let (cached_hit, http, api_key) = {
        let s = state.read().await;
        let hit = s
            .mp_player_cache
            .get(&cache_key)
            .filter(|(ts, _)| is_cache_fresh(*ts, PLAYER_CACHE_TTL, now))
            .map(|(_, rp)| rp.clone());
        (hit, s.http.clone(), s.config.osu_api_key.clone())
    };

    if let Some(rp) = cached_hit {
        return Ok(bot_profile_from_resolved(rp));
    }

    // 3. External lookup (outside AppState locks): osu.ppy.sh web scraper first, API fallback
    let mut resolved_opt: Option<ResolvedPlayer> = None;

    let encoded = percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC).to_string();
    let url = format!("https://osu.ppy.sh/users/{}", encoded);
    if let Ok(resp) = http
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        )
        .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(html) = resp.text().await {
                if let Ok(parsed) = parse_official_userpage_html(&html) {
                    let (pp, rank, acc) = extract_web_user_stats(&html);
                    let country = parsed
                        .country_code
                        .as_deref()
                        .map(utils::country_code_to_byte)
                        .unwrap_or(0);
                    resolved_opt = Some(ResolvedPlayer {
                        userid: parsed.id as i32,
                        username: parsed.username,
                        country,
                        rank,
                        pp,
                        acc,
                    });
                }
            }
        }
    }

    // Fallback or stat-enrichment via osu! API v1 if configured
    if let Some(ref key) = api_key {
        let needs_api = resolved_opt.as_ref().is_none_or(|r| r.pp == 0 && r.rank == 0);
        if needs_api {
            let api_query = resolved_opt
                .as_ref()
                .map(|r| r.userid.to_string())
                .unwrap_or_else(|| trimmed.to_string());
            if let Some(rec) = utils::fetch_user_stats_from_api(&http, key, &api_query).await {
                resolved_opt = Some(ResolvedPlayer {
                    userid: rec.friend_id,
                    username: rec.friend_name,
                    country: rec.country,
                    rank: rec.rank,
                    pp: rec.pp,
                    acc: rec.acc,
                });
            }
        }
    }

    let resolved = resolved_opt.ok_or_else(|| format!("Could not find osu! player '{}'.", trimmed))?;

    // 4. Store in TTL cache
    {
        let mut s = state.write().await;
        s.mp_player_cache.insert(cache_key, (Instant::now(), resolved.clone()));
        s.mp_player_cache
            .insert(resolved.username.to_ascii_lowercase(), (Instant::now(), resolved.clone()));
    }

    Ok(bot_profile_from_resolved(resolved))
}

async fn fetch_user_map_score(
    http: &reqwest::Client,
    api_key: Option<&str>,
    osu_user_id: i32,
    beatmap_id: i64,
    mode: u8,
) -> Option<MimicMapScore> {
    // 1. Prefer official API v1 when key is configured
    if let Some(key) = api_key {
        let url = format!(
            "https://osu.ppy.sh/api/get_scores?k={}&b={}&u={}&m={}&limit=1",
            key, beatmap_id, osu_user_id, mode
        );
        if let Ok(resp) = http.get(&url).send().await {
            if resp.status().is_success() {
                if let Ok(arr) = resp.json::<Vec<serde_json::Value>>().await {
                    if let Some(first) = arr.first() {
                        let parse_u16 = |k: &str| first.get(k).and_then(|v| v.as_str()).and_then(|s| s.parse::<u16>().ok()).unwrap_or(0);
                        let parse_i32 = |k: &str| first.get(k).and_then(|v| v.as_str()).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
                        let parse_u32 = |k: &str| first.get(k).and_then(|v| v.as_str()).and_then(|s| s.parse::<u32>().ok()).unwrap_or(0);
                        let score = parse_i32("score");
                        if score > 0 {
                            return Some(ConstrainedScore {
                                n300: parse_u16("count300"),
                                n100: parse_u16("count100"),
                                n50: parse_u16("count50"),
                                ngeki: parse_u16("countgeki"),
                                nkatu: parse_u16("countkatu"),
                                nmiss: parse_u16("countmiss"),
                                max_combo: parse_u16("maxcombo"),
                                total_score: score,
                                mods: parse_u32("enabled_mods"),
                                perfect: parse_u16("perfect") != 0,
                            });
                        }
                    }
                }
            }
        }
    }

    // 2. Fallback to public web JSON endpoint (no API key required)
    let mode_str = mode_to_ruleset_str(mode as i32);
    let url = format!(
        "https://osu.ppy.sh/beatmaps/{}/scores/users/{}?mode={}",
        beatmap_id, osu_user_id, mode_str
    );
    if let Ok(resp) = http
        .get(&url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
        )
        .header("Accept", "application/json")
        .header("Referer", "https://osu.ppy.sh/")
        .send()
        .await
    {
        if resp.status().is_success() {
            if let Ok(val) = resp.json::<serde_json::Value>().await {
                if let Some(sc) = val.get("score") {
                    let total_score = sc
                        .get("classic_total_score")
                        .or_else(|| sc.get("score"))
                        .or_else(|| sc.get("total_score"))
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0) as i32;
                    if total_score > 0 {
                        let stats = sc.get("statistics");
                        let get_stat = |a: &str, b: &str| {
                            stats
                                .and_then(|st| st.get(a).or_else(|| st.get(b)))
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0) as u16
                        };
                        let mod_items: Vec<BanchoModItem> = sc
                            .get("mods")
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .unwrap_or_default();
                        let (mods_flags, _) = parse_bancho_mods(&mod_items);
                        let nmiss = get_stat("count_miss", "miss");
                        return Some(ConstrainedScore {
                            n300: get_stat("count_300", "great"),
                            n100: get_stat("count_100", "ok"),
                            n50: get_stat("count_50", "meh"),
                            ngeki: get_stat("count_geki", "perfect"),
                            nkatu: get_stat("count_katu", "good"),
                            nmiss,
                            max_combo: sc.get("max_combo").and_then(|v| v.as_u64()).unwrap_or(0) as u16,
                            total_score,
                            mods: mods_flags.bits(),
                            perfect: sc.get("perfect").and_then(|v| v.as_bool()).unwrap_or(nmiss == 0),
                        });
                    }
                }
            }
        }
    }

    None
}

/// Non-blocking background prefetch of map scores for all `MimicPlayer` bots in the active room.
pub fn spawn_prefetch_mimic_scores(state: Arc<RwLock<AppState>>, beatmap_id: i64, mode: u8) {
    if beatmap_id <= 0 {
        return;
    }

    tokio::spawn(async move {
        let (uids_to_fetch, http, api_key) = {
            let s = state.read().await;
            let now = Instant::now();
            let mut uids = Vec::new();
            if let Some(ref room) = s.active_match {
                for slot in &room.slots {
                    if let Some(ref bot) = slot.bot {
                        if let BotSource::MimicPlayer { osu_user_id: Some(uid), .. } = bot.profile.source {
                            let fresh = s
                                .mp_map_score_cache
                                .get(&(uid, beatmap_id))
                                .is_some_and(|(ts, _)| is_cache_fresh(*ts, MAP_SCORE_CACHE_TTL, now));
                            if !fresh && !uids.contains(&uid) {
                                uids.push(uid);
                            }
                        }
                    }
                }
            }
            (uids, s.http.clone(), s.config.osu_api_key.clone())
        };

        for uid in uids_to_fetch {
            let fetched = fetch_user_map_score(&http, api_key.as_deref(), uid, beatmap_id, mode).await;
            let mut s = state.write().await;
            s.mp_map_score_cache.insert((uid, beatmap_id), (Instant::now(), fetched));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::config::Config;

    #[test]
    fn test_pp_to_sr_skill_inference() {
        let low = ResolvedPlayer {
            userid: 100,
            username: "Newbie".to_string(),
            country: 0,
            rank: 400_000,
            pp: 800,
            acc: 93.5,
        };
        let high = ResolvedPlayer {
            userid: 7562902,
            username: "mrekk".to_string(),
            country: 16,
            rank: 1,
            pp: 28_000,
            acc: 98.4,
        };

        let s_low = skill_from_resolved_player(&low);
        let s_high = skill_from_resolved_player(&high);
        assert!(s_high.target_sr > s_low.target_sr + 5.0);
        assert!(s_high.consistency > s_low.consistency);
        assert!(s_high.choke_tendency < s_low.choke_tendency);
        assert!((s_high.accuracy_mean - 98.4).abs() < 1e-6);
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let t0 = Instant::now();
        let ttl = Duration::from_secs(60);
        assert!(is_cache_fresh(t0, ttl, t0 + Duration::from_secs(59)));
        assert!(!is_cache_fresh(t0, ttl, t0 + Duration::from_secs(60)));
        assert!(!is_cache_fresh(t0, ttl, t0 + Duration::from_secs(120)));
    }

    #[tokio::test]
    async fn test_resolve_player_from_local_friends_offline() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "LocalHost").unwrap();
        db::save_friend_stats(
            &conn,
            "LocalHost",
            &db::FriendRecord {
                friend_id: 7562902,
                friend_name: "mrekk".to_string(),
                rank: 1,
                pp: 29_000,
                acc: 98.5,
                country: 16,
                ranked_score: 0,
                total_score: 0,
                playcount: 0,
            },
        )
        .unwrap();

        let state = Arc::new(RwLock::new(AppState::new(conn, Config::default())));
        let bot = resolve_player(&state, "LocalHost", "mrekk").await.unwrap();
        assert_eq!(bot.userid, 7562902);
        assert_eq!(bot.name, "mrekk");
        assert_eq!(bot.rank, 1);
        assert_eq!(bot.pp, 29_000);
        assert_eq!(bot.country, 16);
        assert!(matches!(bot.source, BotSource::MimicPlayer { osu_user_id: Some(7562902), .. }));
    }
}
