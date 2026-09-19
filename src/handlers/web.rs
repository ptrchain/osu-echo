use crate::db;
use crate::packets;
use crate::server::response::Response;
use crate::state::AppState;
use crate::types::direct_response::DirectResponse;
use crate::types::leaderboard::*;
use crate::types::mods::Mods;
use crate::types::score::{LeaderboardEntry, Score};
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

const DEFAULT_CHARTS: &str = concat!(
    "beatmapId:0|beatmapSetId:0|beatmapPlaycount:0|beatmapPasscount:0|approvedDate:0\n",
    "chartId:beatmap|chartUrl:https://osu.ppy.sh/b/0|chartName:Beatmap Ranking|rankBefore:|rankAfter:0|maxComboBefore:|maxComboAfter:0|accuracyBefore:|accuracyAfter:0|rankedScoreBefore:|rankedScoreAfter:0|ppBefore:|ppAfter:0|onlineScoreId:0\n",
    "chartId:overall|chartUrl:https://osu.ppy.sh/u/2|chartName:Overall Ranking|rankBefore:0|rankAfter:0|rankedScoreBefore:0|rankedScoreAfter:0|totalScoreBefore:0|totalScoreAfter:0|maxComboBefore:0|maxComboAfter:0|accuracyBefore:0|accuracyAfter:0|ppBefore:0|ppAfter:0|achievements-new:|onlineScoreId:0",
);

pub async fn handle(
    state: Arc<RwLock<AppState>>,
    sub_path: &str,
    params: &std::collections::HashMap<String, String>,
    method: &hyper::Method,
    headers: &hyper::HeaderMap,
    body: &[u8],
) -> Response {
    match sub_path {
        "/bancho_connect.php" => crate::handlers::cho::bancho_connect(state, params).await,
        "/osu-submit-modular-selector.php" | "/osu-submit-modular.php" => score_sub(state, method, headers, body).await,
        "/osu-getseasonal.php" => get_seasonal_bgs(state).await,
        "/osu-getreplay.php" => get_replay(state, params).await,
        "/osu-osz2-getscores.php" => leaderboard(state, params).await,
        "/osu-search.php" => direct_search(state, params).await,
        "/osu-getfriends.php" => get_friends(state, params).await,
        "/lastfm.php"
        | "/osu-rate.php"
        | "/osu-error.php"
        | "/osu-session.php"
        | "/difficulty-rating"
        | "/osu-markasread.php"
        | "/osu-getbeatmapinfo.php"
        | "/osu-screenshot.php" => Response::empty(),
        _ => Response::empty(),
    }
}

async fn score_sub(state: Arc<RwLock<AppState>>, method: &hyper::Method, headers: &hyper::HeaderMap, body: &[u8]) -> Response {
    {
        let s = state.read().await;
        if s.player.is_none() {
            return Response::empty().with_status(hyper::StatusCode::NOT_FOUND);
        }
    }

    if *method != hyper::Method::POST {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            p.queue.extend_from_slice(&packets::notification("Score submission requires POST (-devserver localhost)."));
        }
        return Response::new(b"error: no".to_vec());
    }

    let content_type = headers.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");

    let decoded = match crate::handlers::score_decode::parse_submission(content_type, body) {
        Ok(d) => d,
        Err(e) => {
            utils::log_error(&format!("Failed to parse/decrypt score submission: {}", e));
            return Response::new(b"error: no".to_vec());
        }
    };

    match crate::handlers::score_submit::process_native_submission(state, decoded).await {
        Ok(charts) => Response::new(charts),
        Err(e) => {
            utils::log_error(&format!("Failed to process score submission: {}", e));
            Response::new(b"error: no".to_vec())
        }
    }
}

async fn get_seasonal_bgs(state: Arc<RwLock<AppState>>) -> Response {
    let s = state.read().await;
    if s.config.seasonal_bgs.is_empty() {
        Response::new(b"[\"\"]".to_vec())
    } else {
        let json = serde_json::to_vec(&s.config.seasonal_bgs).unwrap_or(b"[\"\"]".to_vec());
        Response::new(json)
    }
}

async fn get_replay(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let scoreid: i64 = params.get("c").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mode: i32 = params.get("m").and_then(|v| v.parse().ok()).unwrap_or(0);

    if scoreid > 0 {
        let s = state.read().await;
        if let Some(ref api_key) = s.config.osu_api_key {
            let url = format!("https://osu.ppy.sh/api/get_replay?k={}&s={}&m={}", api_key, scoreid, mode);
            if let Ok(resp) = s.http.get(&url).send().await {
                if resp.status() == 200 {
                    if let Ok(json) = resp.json::<serde_json::Value>().await {
                        if let Some(content) = json.get("content").and_then(|v| v.as_str()) {
                            if let Ok(frames) = utils::string_to_bytes(content) {
                                return Response::new(frames);
                            }
                        }
                    }
                }
            }
        }
        return Response::new(b"error: no".to_vec());
    }

    if scoreid < 0 {
        let real_id = scoreid.unsigned_abs() as i64;
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        if let Ok(Some(score)) = db::get_score_by_id(&db_conn, real_id) {
            if let Some(ref frames_b64) = score.replay_frames {
                if let Ok(frames) = utils::string_to_bytes(frames_b64) {
                    return Response::new(frames);
                }
            }
        }
        return Response::new(b"error: no".to_vec());
    }

    Response::new(b"error: no".to_vec())
}

async fn leaderboard(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let s_read = state.read().await;
    if s_read.player.is_none() {
        return Response::empty().with_status(hyper::StatusCode::NOT_FOUND);
    }

    let mods_val: u32 = params.get("mods").and_then(|v| v.parse().ok()).unwrap_or(0);
    let mode: i32 = params.get("m").and_then(|v| v.parse().ok()).unwrap_or(0);
    let rank_type = LeaderboardTypes::from_i32(params.get("v").and_then(|v| v.parse().ok()).unwrap_or(1));
    let md5 = params.get("c").cloned().unwrap_or_default();
    let setid: i64 = params.get("i").and_then(|v| v.parse().ok()).unwrap_or(0);

    let valid_rank_types = [LeaderboardTypes::Local, LeaderboardTypes::Top, LeaderboardTypes::Mods, LeaderboardTypes::Friends, LeaderboardTypes::Country];
    if !(0..=3).contains(&mode) || !valid_rank_types.contains(&rank_type) {
        return Response::text("0|false");
    }

    let player_name = s_read.player.as_ref().unwrap().name.clone();
    let pp_leaderboard = s_read.config.pp_leaderboard;
    let show_pp_pb = s_read.config.show_pp_for_personal_best;
    let amount = s_read.config.amount_of_scores_on_lb;
    let prev_game_mode = s_read.player.as_ref().map(|p| p.mode).unwrap_or(0);
    drop(s_read);

    let target_game_mode = mode as u8;
    let mods = Mods::from_bits_truncate(mods_val);
    let mut stats_need_update = false;

    {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            if p.mode != target_game_mode {
                p.mode = target_game_mode;
                stats_need_update = true;
            }
        }

        if mods.contains(Mods::RELAX) && s.mode != Some(Mods::RELAX) {
            s.mode = Some(Mods::RELAX);
            s.invalid_mods = Mods::INVALID_RELAX;
            stats_need_update = true;
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::notification("Switched to Relax mode."));
            }
        } else if mods.contains(Mods::AUTOPILOT) && s.mode != Some(Mods::AUTOPILOT) {
            s.mode = Some(Mods::AUTOPILOT);
            s.invalid_mods = Mods::INVALID_AUTOPILOT;
            stats_need_update = true;
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::notification("Switched to Autopilot mode."));
            }
        } else if !mods.intersects(Mods::RELAX | Mods::AUTOPILOT) && s.mode.is_some() {
            s.mode = None;
            s.invalid_mods = Mods::INVALID_STANDARD;
            stats_need_update = true;
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::notification("Switched to Standard mode."));
            }
        }
    }

    if stats_need_update || target_game_mode != prev_game_mode {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, &player_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, &player_name).unwrap_or(0);
        let filter_mod = s.mode;
        let api_key = s.config.osu_daily_api_key.clone();
        let http = s.http.clone();
        drop(db_conn);
        drop(s);

        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.calculate_stats(&scores, filter_mod, playcount);
            let pp = p.pp;
            let current_p_mode = p.mode;
            drop(s_write);

            let rank = if let Some(ref key) = api_key { utils::get_rank_from_daily(&http, key, pp, current_p_mode).await } else { None };

            let mut s_write = state.write().await;
            if let Some(ref mut p) = s_write.player {
                if let Some(r) = rank {
                    p.rank = r;
                }
                p.enqueue_stats();
            }
        }
    }

    let s = state.read().await;
    let current_mode = s.mode;
    let mut lb = Leaderboard::new();

    let mut bmap = {
        let db_conn = s.db.lock().await;
        db::get_beatmap_by_md5(&db_conn, &md5).unwrap_or(None)
    };

    if bmap.is_none() && !md5.is_empty() {
        if let Some(ref api_key) = s.config.osu_api_key {
            if let Some(fetched) = utils::fetch_beatmap_from_api(&s.http, api_key, &[("h", md5.clone())]).await {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &fetched);
                bmap = Some(fetched);
            }
        }
    }

    let Some(bmap) = bmap else {
        utils::log_success(&format!("handled map of setid: {}", setid));
        return Response::new(b"0|false".to_vec());
    };

    lb.bmap = Some(bmap.clone());

    let status = api_to_server_status(bmap.approved);
    if !VALID_LB_STATUSES.contains(&status) {
        utils::log_success(&format!("handled map of setid: {}", setid));
        return Response::new(lb.as_binary(amount, show_pp_pb));
    }

    let is_global = rank_type == LeaderboardTypes::Top || rank_type == LeaderboardTypes::Mods || rank_type == LeaderboardTypes::Country;

    if is_global {
        if let Some(ref api_key) = s.config.osu_api_key {
            let mods_filter = if rank_type == LeaderboardTypes::Mods { Some(mods_val) } else { None };

            let bancho_scores = utils::fetch_scores_from_bancho(&s.http, api_key, bmap.beatmap_id, mode, mods_filter, amount).await;

            if let Some(mut b_scores) = bancho_scores {
                if pp_leaderboard || current_mode.is_some() {
                    if let Some(content) = utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, &bmap).await {
                        if let Ok(parsed_map) = rosu_pp::Beatmap::from_bytes(content.as_bytes()) {
                            for sc in &mut b_scores {
                                utils::calculate_bancho_score_pp(&parsed_map, mode, sc);
                            }
                        }
                    }
                    b_scores.sort_by(|a, b| b.pp.unwrap_or(0.0).partial_cmp(&a.pp.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
                } else {
                    b_scores.sort_by_key(|a| std::cmp::Reverse(a.score.parse::<i64>().unwrap_or(0)));
                }

                for sc in b_scores {
                    lb.scores.push(sc.as_leaderboard_entry(pp_leaderboard || current_mode.is_some()));
                }
            }
        }
    } else if rank_type == LeaderboardTypes::Friends {
        let mut friend_entries: Vec<LeaderboardEntry> = Vec::new();

        let friend_records = {
            let db_conn = s.db.lock().await;
            db::get_friend_records(&db_conn, &player_name).unwrap_or_default()
        };

        {
            let db_conn = s.db.lock().await;
            let mut users_to_check = vec![player_name.clone()];
            for f in &friend_records {
                if !f.friend_name.is_empty() && !users_to_check.iter().any(|u| u.eq_ignore_ascii_case(&f.friend_name)) {
                    users_to_check.push(f.friend_name.clone());
                }
            }

            for u in users_to_check {
                if let Ok(scores) = db::get_scores_on_map(&db_conn, &u, &md5, mode) {
                    let filtered: Vec<&Score> = if let Some(fm) = current_mode {
                        scores.iter().filter(|sc| Mods::from_bits_truncate(sc.mods).intersects(fm)).collect()
                    } else {
                        scores.iter().filter(|sc| !Mods::from_bits_truncate(sc.mods).intersects(Mods::RELAX | Mods::AUTOPILOT)).collect()
                    };

                    for sc in filtered {
                        friend_entries.push(sc.as_leaderboard_entry(pp_leaderboard, show_pp_pb, current_mode));
                    }
                }
            }
        }

        if let Some(ref api_key) = s.config.osu_api_key {
            if bmap.beatmap_id > 0 {
                let parsed_map = if pp_leaderboard || current_mode.is_some() {
                    if let Some(content) = utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, &bmap).await {
                        rosu_pp::Beatmap::from_bytes(content.as_bytes()).ok()
                    } else {
                        None
                    }
                } else {
                    None
                };

                for f in &friend_records {
                    if f.friend_id > 0 {
                        if let Some(mut b_scores) = utils::fetch_user_scores_from_bancho(&s.http, api_key, bmap.beatmap_id, f.friend_id, mode).await {
                            for sc in &mut b_scores {
                                if sc.username.is_empty() && !f.friend_name.is_empty() {
                                    sc.username = f.friend_name.clone();
                                }
                                if let Some(ref map) = parsed_map {
                                    utils::calculate_bancho_score_pp(map, mode, sc);
                                }
                                friend_entries.push(sc.as_leaderboard_entry(pp_leaderboard || current_mode.is_some()));
                            }
                        }
                    }
                }
            }
        }

        friend_entries.sort_by_key(|a| std::cmp::Reverse(a.score));

        let mut seen_users = std::collections::HashSet::new();
        let mut deduped = Vec::new();
        for entry in friend_entries {
            if seen_users.insert(entry.username.to_lowercase()) {
                deduped.push(entry);
            }
        }

        lb.scores = deduped;
    } else if rank_type == LeaderboardTypes::Local {
        let db_conn = s.db.lock().await;
        let scores = db::get_scores_on_map(&db_conn, &player_name, &md5, mode).unwrap_or_default();
        drop(db_conn);

        let filtered: Vec<&Score> = if let Some(fm) = current_mode {
            scores.iter().filter(|sc| Mods::from_bits_truncate(sc.mods).intersects(fm)).collect()
        } else {
            scores.iter().filter(|sc| !Mods::from_bits_truncate(sc.mods).intersects(Mods::RELAX | Mods::AUTOPILOT)).collect()
        };

        let mut sorted = filtered;
        if pp_leaderboard || current_mode.is_some() {
            sorted.sort_by(|a, b| b.pp.unwrap_or(0.0).partial_cmp(&a.pp.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
        } else {
            sorted.sort_by_key(|a| std::cmp::Reverse(a.score));
        }

        for sc in &sorted {
            lb.scores.push(sc.as_leaderboard_entry(pp_leaderboard, show_pp_pb, current_mode));
        }
    }

    let local_scores = {
        let db_conn = s.db.lock().await;
        db::get_scores_on_map(&db_conn, &player_name, &md5, mode).unwrap_or_default()
    };

    let mut personal_candidates: Vec<&Score> = if let Some(fm) = current_mode {
        local_scores.iter().filter(|sc| Mods::from_bits_truncate(sc.mods).intersects(fm)).collect()
    } else {
        local_scores.iter().filter(|sc| !Mods::from_bits_truncate(sc.mods).intersects(Mods::RELAX | Mods::AUTOPILOT)).collect()
    };

    if rank_type == LeaderboardTypes::Mods {
        personal_candidates.retain(|sc| sc.mods == mods_val);
    }

    if pp_leaderboard || current_mode.is_some() {
        personal_candidates.sort_by(|a, b| b.pp.unwrap_or(0.0).partial_cmp(&a.pp.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    } else {
        personal_candidates.sort_by_key(|a| std::cmp::Reverse(a.score));
    }

    if let Some(best) = personal_candidates.first() {
        let personal_entry = best.as_leaderboard_entry(pp_leaderboard, show_pp_pb, current_mode);
        lb.personal_score = Some(personal_entry.clone());

        if is_global {
            let entry_for_list = if pp_leaderboard || current_mode.is_some() { personal_entry } else { best.as_leaderboard_entry(false, false, current_mode) };

            lb.scores.push(entry_for_list);
            lb.scores.sort_by_key(|a| std::cmp::Reverse(a.score));

            if let Some(pos) = lb.scores.iter().position(|s| s.score_id == lb.personal_score.as_ref().unwrap().score_id && s.username == player_name) {
                if pos >= 100 {
                    lb.scores.remove(pos);
                }
            }
        } else if rank_type == LeaderboardTypes::Friends
            && !lb.scores.iter().any(|s| s.username.eq_ignore_ascii_case(&player_name))
        {
            lb.scores.push(personal_entry);
            lb.scores.sort_by_key(|a| std::cmp::Reverse(a.score));
        }
    }

    lb.scores.truncate(amount as usize);

    utils::log_success(&format!("handled map of setid: {}", setid));
    Response::new(lb.as_binary(amount, show_pp_pb))
}

async fn get_friends(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let player_name = if let Some(u) = params.get("u") {
        utils::url_decode(u)
    } else {
        let s = state.read().await;
        s.player.as_ref().map(|p| p.name.clone()).unwrap_or_default()
    };

    let s = state.read().await;
    let db_conn = s.db.lock().await;
    let mut friend_ids = db::get_friend_ids(&db_conn, &player_name).unwrap_or_default();
    drop(db_conn);
    drop(s);

    if !friend_ids.contains(&3) {
        friend_ids.push(3);
    }
    if !friend_ids.contains(&4) {
        friend_ids.push(4);
    }

    let mut resp_str = String::new();
    for id in friend_ids {
        resp_str.push_str(&id.to_string());
        resp_str.push('\n');
    }
    Response::text(&resp_str)
}

#[derive(Debug, serde::Deserialize)]
struct CatboyBeatmap {
    #[serde(rename = "DiffName")]
    diff_name: Option<String>,
    #[serde(rename = "Mode")]
    mode: Option<i32>,
}

#[derive(Debug, serde::Deserialize)]
struct CatboyBeatmapSet {
    #[serde(rename = "SetID")]
    set_id: i64,
    #[serde(rename = "RankedStatus")]
    ranked_status: i32,
    #[serde(rename = "Artist")]
    artist: Option<String>,
    #[serde(rename = "Title")]
    title: Option<String>,
    #[serde(rename = "Creator")]
    creator: Option<String>,
    #[serde(rename = "LastUpdate")]
    last_update: Option<String>,
    #[serde(rename = "ChildrenBeatmaps")]
    children_beatmaps: Option<Vec<CatboyBeatmap>>,
}

async fn search_catboy(http: &reqwest::Client, query: &str, mode: i32, ranking_status: i32) -> Option<Vec<u8>> {
    let mut url = reqwest::Url::parse("https://catboy.best/api/search").ok()?;
    {
        let mut pairs = url.query_pairs_mut();
        if !["Newest", "Top Rated", "Most Played"].contains(&query) && !query.is_empty() {
            pairs.append_pair("q", query);
        }
        if mode != -1 {
            pairs.append_pair("mode", &mode.to_string());
        }
        if ranking_status == 3 {
            pairs.append_pair("status", "3");
        } else if ranking_status == 8 {
            pairs.append_pair("status", "-2");
        }
        pairs.append_pair("amount", "100");
    }

    let resp = http.get(url).send().await.ok()?;
    if resp.status() != 200 {
        return None;
    }

    let sets: Vec<CatboyBeatmapSet> = resp.json().await.ok()?;
    let mut direct_resp = DirectResponse::new();

    for set in sets {
        let diffs: Vec<String> = set
            .children_beatmaps
            .unwrap_or_default()
            .into_iter()
            .map(|b| {
                let name = b.diff_name.unwrap_or_else(|| "Normal".to_string());
                let m = b.mode.unwrap_or(0);
                format!("{}@{}", name, m)
            })
            .collect();

        direct_resp.entries.push(crate::types::direct_response::DirectEntry {
            id: set.set_id,
            artist: set.artist.unwrap_or_default(),
            title: set.title.unwrap_or_default(),
            creator: set.creator.unwrap_or_default(),
            ranked: set.ranked_status,
            last_updated: set.last_update.unwrap_or_default(),
            diffs,
        });
    }

    Some(direct_resp.as_binary())
}

async fn direct_search(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let query = params.get("q").map(|q| utils::url_decode(q)).unwrap_or_default();
    let mode: i32 = params.get("m").and_then(|v| v.parse().ok()).unwrap_or(-1);
    let ranking_status: i32 = params.get("r").and_then(|v| v.parse().ok()).unwrap_or(0);

    let (prefix, http, osu_username, osu_password) = {
        let s = state.read().await;
        (s.config.command_prefix.clone(), s.http.clone(), s.config.osu_username.clone(), s.config.osu_password.clone())
    };

    if query.starts_with(&prefix) {
        let msg = "Commands have moved to in-game chat!\nType your command in #osu or PM BanchoBot/Tillerino (e.g. !help, /np, !r).";
        let resp = DirectResponse::from_str(msg, -2);
        return Response::new(resp.as_binary());
    }

    // Catboy as main search mirror
    if let Some(body) = search_catboy(&http, &query, mode, ranking_status).await {
        utils::log_success(&format!("Catboy mirror: maps loaded for query: `{}`!", query));
        return Response::new(body);
    }

    // Fallback to official osu! search if credentials are provided
    if let (Some(username), Some(password)) = (osu_username, osu_password) {
        if let Ok(mut url) = reqwest::Url::parse("https://osu.ppy.sh/web/osu-search.php") {
            {
                let mut pairs = url.query_pairs_mut();
                pairs.append_pair("u", &username);
                pairs.append_pair("h", &password);
                if !["Newest", "Top Rated", "Most Played"].contains(&query.as_str()) && !query.is_empty() {
                    pairs.append_pair("q", &query);
                }
                if mode != -1 {
                    pairs.append_pair("m", &mode.to_string());
                }
                pairs.append_pair("r", &ranking_status.to_string());
            }

            if let Ok(resp) = http.get(url).send().await {
                if let Ok(body) = resp.bytes().await {
                    utils::log_success(&format!("Official osu!: maps loaded for query: `{}`!", query));
                    return Response::new(body.to_vec());
                }
            }
        }
    }

    utils::log_error("Failed to load maps from Catboy mirror and official fallback");
    Response::new(b"0".to_vec())
}

pub async fn handle_download(_state: Arc<RwLock<AppState>>, setid: i64) -> Response {
    if setid <= 0 {
        return Response::empty();
    }

    Response::redirect(&format!("https://catboy.best/d/{}", setid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use simple_rijndael::impls::RijndaelCbc;
    use simple_rijndael::paddings::Pkcs7Padding;

    #[tokio::test]
    async fn test_score_sub_post_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Alice").unwrap();

        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.file_md5 = "0123456789abcdef0123456789abcdef".to_string();
        bmap.beatmap_id = 999;
        bmap.beatmapset_id = 888;
        bmap.approved = 1;
        bmap.file_content = Some("osu file format v14\n[General]\nMode: 0\n[Difficulty]\nHPDrainRate:5\nCircleSize:5\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n[TimingPoints]\n0,500,4,1,0,100,1,0\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n".to_string());
        db::insert_beatmap(&conn, &bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Alice".to_string()));

        let shared_state = Arc::new(RwLock::new(app_state));

        let osuver = "20260912";
        let key = format!("osu!-scoreburgr---------{}", osuver).into_bytes();
        let iv = vec![42u8; 32];
        let plaintext = format!("{}:Alice:{}:1:0:0:0:0:0:10000:1:True:S:0:True:0:260912120000:20260912", bmap.file_md5, "checksum12345");

        let cipher = RijndaelCbc::<Pkcs7Padding>::new(&key, 32).unwrap();
        let encrypted = cipher.encrypt(&iv, plaintext.into_bytes()).unwrap();

        let b64 = base64::engine::general_purpose::STANDARD;
        let iv_b64 = b64.encode(&iv);
        let score_b64 = b64.encode(&encrypted);

        let body = format!(
            "--boundary\r\nContent-Disposition: form-data; name=\"osuver\"\r\n\r\n{}\r\n\
--boundary\r\nContent-Disposition: form-data; name=\"iv\"\r\n\r\n{}\r\n\
--boundary\r\nContent-Disposition: form-data; name=\"score\"\r\n\r\n{}\r\n\
--boundary\r\nContent-Disposition: form-data; name=\"score\"; filename=\"replay.osr\"\r\n\r\nfake_replay_frames\r\n\
--boundary--\r\n",
            osuver, iv_b64, score_b64
        );

        let mut headers = hyper::HeaderMap::new();
        headers.insert("content-type", hyper::header::HeaderValue::from_static("multipart/form-data; boundary=boundary"));

        let params = std::collections::HashMap::new();
        let resp = handle(shared_state.clone(), "/osu-submit-modular-selector.php", &params, &hyper::Method::POST, &headers, body.as_bytes()).await;

        let resp_str = String::from_utf8(resp.body).unwrap();
        assert!(resp_str.contains("beatmapId:999"));
        assert!(resp_str.contains("chartId:beatmap"));
        assert!(resp_str.contains("chartId:overall"));

        let s = shared_state.read().await;
        let p = s.player.as_ref().unwrap();
        assert_eq!(p.playcount, 1);

        let pkts = packets::split_packets(&p.queue);
        let notif_pkt = pkts.iter().find(|pkt| pkt.id == packets::PacketId::ChoNotification as u16).expect("Score submission notification");
        let mut r = packets::PacketReader::new(notif_pkt.payload);
        let msg = r.read_string().expect("notif message");
        assert!(msg.starts_with("Score submitted"), "Notification must be minimal score submitted format");
        assert!(!msg.contains('\n'), "Notification must be a clean single line");

        let recent_msg_pkt = pkts.iter().find(|pkt| pkt.id == packets::PacketId::ChoSendMessage as u16).expect("#recent message");
        let mut r = packets::PacketReader::new(recent_msg_pkt.payload);
        let _client = r.read_string().unwrap();
        let _content = r.read_string().unwrap();
        let target = r.read_string().unwrap();
        assert_eq!(target, "#recent");
    }

    #[tokio::test]
    async fn test_leaderboard_local_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Bob").unwrap();

        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.file_md5 = "fedcba9876543210fedcba9876543210".to_string();
        bmap.beatmap_id = 1234;
        bmap.beatmapset_id = 5678;
        bmap.approved = 1;
        bmap.title = "Test Song".to_string();
        bmap.artist = "Test Artist".to_string();
        db::insert_beatmap(&conn, &bmap).unwrap();

        let score = Score {
            mode: 0,
            md5: bmap.file_md5.clone(),
            name: "Bob".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 1_234_567,
            max_combo: 500,
            perfect: true,
            mods: 0,
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(250.0),
            replay_md5: None,
            scoreid: Some(101),
            replay_frames: Some("fake_frames".to_string()),
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        db::insert_score(&conn, &score, "ranked").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Bob".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params = std::collections::HashMap::new();
        params.insert("c".to_string(), bmap.file_md5.clone());
        params.insert("m".to_string(), "0".to_string());
        params.insert("v".to_string(), "0".to_string());

        let headers = hyper::HeaderMap::new();
        let resp = handle(shared_state.clone(), "/osu-osz2-getscores.php", &params, &hyper::Method::GET, &headers, &[]).await;

        let resp_str = String::from_utf8(resp.body).unwrap();
        // Bancho format check: 2|false|1234|5678|...
        assert!(resp_str.starts_with("2|false|1234|5678|1\n"));
        assert!(resp_str.contains("Test Artist|Test Song"));
        assert!(resp_str.contains("Bob"));
    }

    #[tokio::test]
    async fn test_leaderboard_unranked_map() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Charlie").unwrap();

        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.file_md5 = "unranked_md5_hash_1234567890abcdef".to_string();
        bmap.beatmap_id = 4321;
        bmap.beatmapset_id = 8765;
        bmap.approved = 0;
        db::insert_beatmap(&conn, &bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Charlie".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params = std::collections::HashMap::new();
        params.insert("c".to_string(), bmap.file_md5.clone());
        params.insert("m".to_string(), "0".to_string());
        params.insert("v".to_string(), "1".to_string());

        let headers = hyper::HeaderMap::new();
        let resp = handle(shared_state.clone(), "/osu-osz2-getscores.php", &params, &hyper::Method::GET, &headers, &[]).await;

        let resp_str = String::from_utf8(resp.body).unwrap();
        assert_eq!(resp_str, "0|false");
    }

    #[tokio::test]
    async fn test_leaderboard_selected_mods_filtering() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Dave").unwrap();

        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.file_md5 = "mods_test_md5_hash_1234567890abcdef".to_string();
        bmap.beatmap_id = 777;
        bmap.beatmapset_id = 888;
        bmap.approved = 1;
        db::insert_beatmap(&conn, &bmap).unwrap();

        let score_hd = Score {
            mode: 0,
            md5: bmap.file_md5.clone(),
            name: "Dave".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 1_000_000,
            max_combo: 500,
            perfect: true,
            mods: 8,
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(300.0),
            replay_md5: None,
            scoreid: Some(201),
            replay_frames: Some("frames".to_string()),
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        db::insert_score(&conn, &score_hd, "ranked").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Dave".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params_hr = std::collections::HashMap::new();
        params_hr.insert("c".to_string(), bmap.file_md5.clone());
        params_hr.insert("m".to_string(), "0".to_string());
        params_hr.insert("v".to_string(), "2".to_string());
        params_hr.insert("mods".to_string(), "16".to_string());

        let headers = hyper::HeaderMap::new();
        let resp_hr = handle(shared_state.clone(), "/osu-osz2-getscores.php", &params_hr, &hyper::Method::GET, &headers, &[]).await;
        let str_hr = String::from_utf8(resp_hr.body).unwrap();
        assert!(str_hr.starts_with("2|false|777|888|0\n"));

        let mut params_hd = std::collections::HashMap::new();
        params_hd.insert("c".to_string(), bmap.file_md5.clone());
        params_hd.insert("m".to_string(), "0".to_string());
        params_hd.insert("v".to_string(), "2".to_string());
        params_hd.insert("mods".to_string(), "8".to_string());

        let resp_hd = handle(shared_state.clone(), "/osu-osz2-getscores.php", &params_hd, &hyper::Method::GET, &headers, &[]).await;
        let str_hd = String::from_utf8(resp_hd.body).unwrap();
        assert!(str_hd.starts_with("2|false|777|888|1\n"));
        assert!(str_hd.contains("Dave"));
    }

    #[tokio::test]
    async fn test_leaderboard_binary_generation_with_bancho_scores() {
        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.beatmap_id = 1001;
        bmap.beatmapset_id = 2002;
        bmap.approved = 1;
        bmap.title = "Freedom Dive".to_string();
        bmap.artist = "xi".to_string();

        let mut lb = Leaderboard::new();
        lb.bmap = Some(bmap);

        let json_bancho = serde_json::json!({
            "score_id": "999999",
            "username": "Cookiezi",
            "score": "9999999",
            "maxcombo": "2000",
            "count50": "0",
            "count100": "0",
            "count300": "1500",
            "countmiss": "0",
            "countkatu": "0",
            "countgeki": "0",
            "perfect": "1",
            "enabled_mods": "24",
            "user_id": "124",
            "date": "2023-01-01 00:00:00",
            "replay_available": "1",
            "pp": "800.0"
        });

        let b_score = crate::types::score::BanchoScore::from_json(&json_bancho).unwrap();
        let bancho_entry = b_score.as_leaderboard_entry(true);

        let local_entry = crate::types::score::LeaderboardEntry {
            score_id: -1,
            username: "LocalPlayer".to_string(),
            score: 750,
            maxcombo: 1800,
            count50: 0,
            count100: 5,
            count300: 1495,
            countmiss: 0,
            countkatu: 0,
            countgeki: 0,
            perfect: 1,
            enabled_mods: 0,
            user_id: 2,
            time: 1700000000,
            replay_available: 1,
        };

        lb.personal_score = Some(local_entry.clone());
        lb.scores.push(bancho_entry);
        lb.scores.push(local_entry);

        lb.scores.sort_by_key(|a| std::cmp::Reverse(a.score));

        let bin = lb.as_binary(100, false);
        let s = String::from_utf8(bin).unwrap();
        let lines: Vec<&str> = s.lines().collect();

        // Wire format: header, offset, song title, rating, personal best, and ranked scores
        assert_eq!(lines[0], "2|false|1001|2002|2");
        assert_eq!(lines[1], "0");
        assert_eq!(lines[2], "[bold:0,size:20]xi|Freedom Dive");
        assert_eq!(lines[3], "10.0");
        assert!(lines[4].contains("LocalPlayer"));
        assert!(lines[4].contains("|2|"));
        assert!(lines[5].contains("Cookiezi"));
        assert!(lines[5].contains("|1|"));
        assert!(lines[6].contains("LocalPlayer"));
        assert!(lines[6].contains("|2|"));
    }

    #[tokio::test]
    async fn test_get_friends_endpoint() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "FriendTester").unwrap();
        db::add_friend(&conn, "FriendTester", 12345, "Buddy1").unwrap();
        db::add_friend(&conn, "FriendTester", 67890, "Buddy2").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("FriendTester".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params = std::collections::HashMap::new();
        params.insert("u".to_string(), "FriendTester".to_string());

        let headers = hyper::HeaderMap::new();
        let resp = handle(shared_state, "/osu-getfriends.php", &params, &hyper::Method::GET, &headers, &[]).await;
        let body_str = String::from_utf8(resp.body).unwrap();
        assert!(body_str.contains("3\n"), "BanchoBot (3) must be in friends response");
        assert!(body_str.contains("4\n"), "Tillerino (4) must be in friends response");
        assert!(body_str.contains("12345\n"));
        assert!(body_str.contains("67890\n"));
    }

    #[tokio::test]
    async fn test_leaderboard_friends_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "MainPlayer").unwrap();
        db::ensure_profile(&conn, "Bestie").unwrap();
        db::add_friend(&conn, "MainPlayer", 8888, "Bestie").unwrap();

        let mut bmap = crate::types::beatmap::Beatmap::blank();
        bmap.beatmap_id = 1234;
        bmap.beatmapset_id = 5678;
        bmap.artist = "Camellia".to_string();
        bmap.title = "Ghost".to_string();
        bmap.version = "Insane".to_string();
        bmap.creator = "Mapper".to_string();
        bmap.file_md5 = "ghostmd5".to_string();
        bmap.diff_overall = 9.0;
        bmap.diff_approach = 9.0;
        bmap.diff_size = 4.0;
        bmap.diff_drain = 6.0;
        bmap.mode = 0;
        bmap.approved = 1;
        bmap.bpm = 220.0;
        db::insert_beatmap(&conn, &bmap).unwrap();

        // Add a score for Bestie and a score for MainPlayer
        let score_bestie = Score {
            mode: 0,
            md5: "ghostmd5".to_string(),
            name: "Bestie".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 50,
            nkatu: 0,
            nmiss: 0,
            score: 5000000,
            max_combo: 600,
            perfect: true,
            mods: 0,
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(350.0),
            replay_md5: None,
            scoreid: Some(101),
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        db::insert_score(&conn, &score_bestie, "ranked").unwrap();

        let score_main = Score {
            mode: 0,
            md5: "ghostmd5".to_string(),
            name: "MainPlayer".to_string(),
            n300: 290,
            n100: 10,
            n50: 0,
            ngeki: 40,
            nkatu: 0,
            nmiss: 0,
            score: 4500000,
            max_combo: 600,
            perfect: true,
            mods: 0,
            time: 1700000010,
            acc: Some(98.5),
            pp: Some(300.0),
            replay_md5: None,
            scoreid: Some(102),
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        db::insert_score(&conn, &score_main, "ranked").unwrap();

        let mut config = crate::types::config::Config::default();
        config.pp_leaderboard = true;
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("MainPlayer".to_string());
        player.userid = 2;
        app_state.player = Some(player);
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params = std::collections::HashMap::new();
        params.insert("c".to_string(), "ghostmd5".to_string());
        params.insert("m".to_string(), "0".to_string());
        params.insert("v".to_string(), "3".to_string()); // Friends leaderboard

        let headers = hyper::HeaderMap::new();
        let resp = handle(shared_state, "/osu-osz2-getscores.php", &params, &hyper::Method::GET, &headers, &[]).await;
        assert_eq!(resp.status, hyper::StatusCode::OK);
        let body_str = String::from_utf8(resp.body).unwrap();
        assert!(!body_str.starts_with("0|false"));
        assert!(body_str.contains("Bestie"), "Friend score must appear on friend leaderboard");
        assert!(body_str.contains("MainPlayer"), "Player personal score must appear on friend leaderboard");
    }

    #[tokio::test]
    async fn test_direct_search_command_guidance() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let config = crate::types::config::Config::default();
        let app_state = AppState::new(conn, config);
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params = std::collections::HashMap::new();
        params.insert("q".to_string(), "!help".to_string());

        let headers = hyper::HeaderMap::new();
        let resp = handle(shared_state, "/osu-search.php", &params, &hyper::Method::GET, &headers, &[]).await;
        assert_eq!(resp.status, hyper::StatusCode::OK);
        // Direct response starts with set count / cards
        assert!(!resp.body.is_empty());
    }

    #[tokio::test]
    async fn test_handle_download_catboy() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let config = crate::types::config::Config::default();
        let app_state = AppState::new(conn, config);
        let shared_state = Arc::new(RwLock::new(app_state));

        let resp = handle_download(shared_state, 12345).await;
        assert_eq!(resp.status, hyper::StatusCode::MOVED_PERMANENTLY);
        let location = resp.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("location")).map(|(_, v)| v.as_str()).unwrap();
        assert_eq!(location, "https://catboy.best/d/12345");
    }
}
