use crate::db;
use crate::packets;
use crate::server::response::Response;
use crate::state::AppState;
use crate::types::direct_response::DirectResponse;
use crate::types::leaderboard::*;
use crate::types::mods::Mods;
use crate::types::score::Score;
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
        "/lastfm.php"
        | "/osu-rate.php"
        | "/osu-error.php"
        | "/osu-session.php"
        | "/difficulty-rating"
        | "/osu-markasread.php"
        | "/osu-getfriends.php"
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
            p.queue.extend_from_slice(&packets::notification("Score submission requires POST. Reconnect using -devserver localhost."));
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

    let valid_rank_types = [LeaderboardTypes::Local, LeaderboardTypes::Top, LeaderboardTypes::Mods];
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
                p.queue.extend_from_slice(&packets::notification("Mode was switched to rx!"));
            }
        } else if mods.contains(Mods::AUTOPILOT) && s.mode != Some(Mods::AUTOPILOT) {
            s.mode = Some(Mods::AUTOPILOT);
            s.invalid_mods = Mods::INVALID_AUTOPILOT;
            stats_need_update = true;
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::notification("Mode was switched to ap!"));
            }
        } else if !mods.intersects(Mods::RELAX | Mods::AUTOPILOT) && s.mode.is_some() {
            s.mode = None;
            s.invalid_mods = Mods::INVALID_STANDARD;
            stats_need_update = true;
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::notification("Mode was switched to vanilla!"));
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

            let rank = if let Some(ref key) = api_key {
                utils::get_rank_from_daily(&http, key, pp, current_p_mode).await
            } else {
                None
            };

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

    let is_global = rank_type == LeaderboardTypes::Top || rank_type == LeaderboardTypes::Mods;

    if is_global {
        if let Some(ref api_key) = s.config.osu_api_key {
            let mods_filter = if rank_type == LeaderboardTypes::Mods {
                Some(mods_val)
            } else {
                None
            };

            let bancho_scores = utils::fetch_scores_from_bancho(
                &s.http,
                api_key,
                bmap.beatmap_id,
                mode,
                mods_filter,
                amount,
            )
            .await;

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

    // Personal score retrieval & integration
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
            let entry_for_list = if pp_leaderboard || current_mode.is_some() {
                personal_entry
            } else {
                best.as_leaderboard_entry(false, false, current_mode)
            };

            lb.scores.push(entry_for_list);
            lb.scores.sort_by_key(|a| std::cmp::Reverse(a.score));

            if let Some(pos) = lb.scores.iter().position(|s| s.score_id == lb.personal_score.as_ref().unwrap().score_id && s.username == player_name) {
                if pos >= 100 {
                    lb.scores.remove(pos);
                }
            }
        }
    }

    lb.scores.truncate(amount as usize);

    utils::log_success(&format!("handled map of setid: {}", setid));
    Response::new(lb.as_binary(amount, show_pp_pb))
}

async fn direct_search(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let query = params.get("q").map(|q| utils::url_decode(q)).unwrap_or_default();
    let mode: i32 = params.get("m").and_then(|v| v.parse().ok()).unwrap_or(-1);
    let ranking_status: i32 = params.get("r").and_then(|v| v.parse().ok()).unwrap_or(0);

    let s = state.read().await;

    // In-game command execution via osu!direct search bar; clicking card confirms execution (-1)
    if query.starts_with(&s.config.command_prefix) {
        let query_no_prefix = query.strip_prefix(&s.config.command_prefix).unwrap_or(&query);
        let parts: Vec<&str> = query_no_prefix.split_whitespace().collect();

        if parts.is_empty() || !s.commands.contains_key(parts[0]) {
            let resp = DirectResponse::from_str("command not found!", -2);
            return Response::new(resp.as_binary());
        }

        let cmd_name = parts[0].to_string();
        let args: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
        let command = s.commands.get(&cmd_name).cloned();
        drop(s);

        if let Some(cmd) = command {
            if cmd.confirm_with_user {
                let mut s = state.write().await;
                let mut cmd = cmd.clone();
                cmd.args = args.clone();
                s.current_cmd = Some(cmd);

                let msg = format!("click me to execute!\nloaded command: {}\nfollowing args: {:?}", cmd_name, args);
                let resp = DirectResponse::from_str(&msg, -1);
                return Response::new(resp.as_binary());
            } else {
                let result = (cmd.func)(state.clone(), args).await;
                if let Some(data) = result {
                    return Response::new(data);
                }
                return Response::new(b"0".to_vec());
            }
        }

        return Response::new(b"0".to_vec());
    }

    if s.config.osu_username.is_none() || s.config.osu_password.is_none() {
        return Response::new(b"0".to_vec());
    }

    let username = s.config.osu_username.as_deref().unwrap();
    let password = s.config.osu_password.as_deref().unwrap();

    let mut url = reqwest::Url::parse("https://osu.ppy.sh/web/osu-search.php").unwrap();
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("u", username);
        pairs.append_pair("h", password);
        if !["Newest", "Top Rated", "Most Played"].contains(&query.as_str()) {
            pairs.append_pair("q", &query);
        }
        if mode != -1 {
            pairs.append_pair("m", &mode.to_string());
        }
        pairs.append_pair("r", &ranking_status.to_string());
    }

    match s.http.get(url).send().await {
        Ok(resp) => {
            if let Ok(body) = resp.bytes().await {
                utils::log_success(&format!("maps loaded for query: `{}` !", query));
                Response::new(body.to_vec())
            } else {
                Response::new(b"0".to_vec())
            }
        }
        Err(_) => {
            utils::log_error("mirror currently down");
            Response::new(b"0".to_vec())
        }
    }
}

pub async fn handle_download(state: Arc<RwLock<AppState>>, setid: i64) -> Response {
    if setid == -1 {
        let cmd = {
            let mut s = state.write().await;
            s.current_cmd.take()
        };
        if let Some(cmd) = cmd {
            let args = cmd.args.clone();
            (cmd.func)(state, args).await;
        }
        return Response::empty();
    }

    Response::redirect(&format!("https://osu.gatari.pw/d/{}", setid))
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
        params.insert("v".to_string(), "0".to_string()); // Local

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
        bmap.approved = 0; // pending / unranked
        db::insert_beatmap(&conn, &bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Charlie".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut params = std::collections::HashMap::new();
        params.insert("c".to_string(), bmap.file_md5.clone());
        params.insert("m".to_string(), "0".to_string());
        params.insert("v".to_string(), "1".to_string()); // Global

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

        // Insert score with HD (8)
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

        // Query with HR (16) on Selected Mods (v=2) - should not match HD score
        let mut params_hr = std::collections::HashMap::new();
        params_hr.insert("c".to_string(), bmap.file_md5.clone());
        params_hr.insert("m".to_string(), "0".to_string());
        params_hr.insert("v".to_string(), "2".to_string());
        params_hr.insert("mods".to_string(), "16".to_string());

        let headers = hyper::HeaderMap::new();
        let resp_hr = handle(shared_state.clone(), "/osu-osz2-getscores.php", &params_hr, &hyper::Method::GET, &headers, &[]).await;
        let str_hr = String::from_utf8(resp_hr.body).unwrap();
        // Should have 0 scores in count
        assert!(str_hr.starts_with("2|false|777|888|0\n"));

        // Query with HD (8) on Selected Mods (v=2) - should match!
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

        // Line 0: status header
        assert_eq!(lines[0], "2|false|1001|2002|2");
        // Line 1: offset
        assert_eq!(lines[1], "0");
        // Line 2: song title
        assert_eq!(lines[2], "[bold:0,size:20]xi|Freedom Dive");
        // Line 3: rating
        assert_eq!(lines[3], "10.0");
        // Line 4: personal score line (LocalPlayer should be rank 2)
        assert!(lines[4].contains("LocalPlayer"));
        assert!(lines[4].contains("|2|"));
        // Line 5: first score on leaderboard (Cookiezi at rank 1)
        assert!(lines[5].contains("Cookiezi"));
        assert!(lines[5].contains("|1|"));
        // Line 6: second score on leaderboard (LocalPlayer at rank 2)
        assert!(lines[6].contains("LocalPlayer"));
        assert!(lines[6].contains("|2|"));
    }
}
