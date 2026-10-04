use crate::db;
use crate::handlers::medals;
use crate::handlers::score_decode::DecodedSubmission;
use crate::packets;
use crate::state::AppState;
use crate::types::beatmap::Beatmap;
use crate::types::leaderboard;
use crate::types::medal::get_medal_by_id;
use crate::types::mods::Mods;
use crate::types::replay::Replay;
use crate::types::score::Score;
use crate::utils;
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Default)]
pub struct ProfileStats {
    pub rank: i32,
    pub pp: f64,
    pub acc: f64,
    pub ranked_score: i64,
    pub total_score: i64,
    pub max_combo: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum ScoreMetric {
    Score(i64),
    Pp(f64),
}

impl ScoreMetric {
    pub fn is_greater_than(&self, other: &Self) -> bool {
        match (self, other) {
            (ScoreMetric::Score(a), ScoreMetric::Score(b)) => a > b,
            (ScoreMetric::Pp(a), ScoreMetric::Pp(b)) => a > b,
            _ => false,
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn calculate_map_ranks(
    http: &reqwest::Client,
    api_key: Option<&str>,
    bmap: &Beatmap,
    mode: i32,
    server_mode: Option<Mods>,
    pp_leaderboard: bool,
    parsed_map: Option<&rosu_pp::Beatmap>,
    all_local_scores: &[Score],
    player_name: &str,
    new_score: &Score,
) -> (Option<Score>, Option<i32>, Option<i32>, Option<i32>) {
    let get_score_metric = |sc: &Score| -> ScoreMetric {
        if pp_leaderboard || server_mode.is_some() {
            ScoreMetric::Pp(sc.pp.unwrap_or(0.0))
        } else {
            ScoreMetric::Score(sc.score)
        }
    };

    let is_valid_mode = |mods: u32| -> bool {
        let m = Mods::from_bits_truncate(mods);
        if let Some(fm) = server_mode {
            m.intersects(fm)
        } else {
            !m.intersects(Mods::RELAX | Mods::AUTOPILOT)
        }
    };

    let player_prev_scores: Vec<&Score> = all_local_scores
        .iter()
        .filter(|sc| sc.name.eq_ignore_ascii_case(player_name) && is_valid_mode(sc.mods))
        .collect();

    let max_combo_before = player_prev_scores.iter().map(|s| s.max_combo).max();

    let previous = player_prev_scores
        .into_iter()
        .max_by(|a, b| {
            if pp_leaderboard || server_mode.is_some() {
                a.pp.unwrap_or(0.0).partial_cmp(&b.pp.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal)
            } else {
                a.score.cmp(&b.score)
            }
        })
        .cloned();

    let mut competitor_metrics: Vec<ScoreMetric> = Vec::new();

    if let Some(key) = api_key {
        if bmap.beatmap_id > 0 {
            if let Some(mut bancho_scores) = utils::fetch_scores_from_bancho(http, key, bmap.beatmap_id, mode, None, 50).await {
                if pp_leaderboard || server_mode.is_some() {
                    if let Some(map) = parsed_map {
                        for sc in &mut bancho_scores {
                            utils::calculate_bancho_score_pp(map, mode, sc);
                        }
                    }
                }
                for sc in bancho_scores {
                    if !sc.username.eq_ignore_ascii_case(player_name) {
                        if pp_leaderboard || server_mode.is_some() {
                            competitor_metrics.push(ScoreMetric::Pp(sc.pp.unwrap_or(0.0)));
                        } else {
                            competitor_metrics.push(ScoreMetric::Score(sc.score.parse::<i64>().unwrap_or(0)));
                        }
                    }
                }
            }
        }
    }

    let mut other_players: std::collections::HashMap<String, &Score> = std::collections::HashMap::new();
    for sc in all_local_scores {
        if !sc.name.eq_ignore_ascii_case(player_name) && is_valid_mode(sc.mods) {
            let entry = other_players.entry(sc.name.to_lowercase()).or_insert(sc);
            let curr_better = if pp_leaderboard || server_mode.is_some() {
                sc.pp.unwrap_or(0.0) > entry.pp.unwrap_or(0.0)
            } else {
                sc.score > entry.score
            };
            if curr_better {
                *entry = sc;
            }
        }
    }

    for (_, sc) in other_players {
        competitor_metrics.push(get_score_metric(sc));
    }

    competitor_metrics.sort_by(|a, b| {
        if b.is_greater_than(a) {
            std::cmp::Ordering::Greater
        } else if a.is_greater_than(b) {
            std::cmp::Ordering::Less
        } else {
            std::cmp::Ordering::Equal
        }
    });

    let rank_before = if let Some(ref prev) = previous {
        let prev_m = get_score_metric(prev);
        let better_count = competitor_metrics.iter().filter(|m| m.is_greater_than(&prev_m)).count();
        if better_count < 50 {
            Some((better_count + 1) as i32)
        } else {
            None
        }
    } else {
        None
    };

    let new_m = get_score_metric(new_score);
    let better_count = competitor_metrics.iter().filter(|m| m.is_greater_than(&new_m)).count();
    let rank_after = if better_count < 50 {
        Some((better_count + 1) as i32)
    } else {
        None
    };

    (previous, rank_before, rank_after, max_combo_before)
}

#[allow(clippy::too_many_arguments)]
pub fn build_charts(
    bmap: &Beatmap,
    score: &Score,
    before: &ProfileStats,
    after: &ProfileStats,
    previous: Option<&Score>,
    rank_before: Option<i32>,
    rank_after: Option<i32>,
    max_combo_before: Option<i32>,
    playcount: i32,
    achievements_new: Option<&str>,
) -> Vec<u8> {
    let meta = format!(
        "beatmapId:{}|beatmapSetId:{}|beatmapPlaycount:{}|beatmapPasscount:{}|approvedDate:0",
        bmap.beatmap_id, bmap.beatmapset_id, playcount, playcount
    );

    let prev_score_str = previous.map(|s| s.score.to_string()).unwrap_or_default();
    let effective_combo_before = max_combo_before.or_else(|| previous.map(|s| s.max_combo));
    let (prev_max_combo, curr_max_combo) = match effective_combo_before {
        Some(prev_max) => (prev_max.to_string(), prev_max.max(score.max_combo).to_string()),
        None => (String::new(), score.max_combo.to_string()),
    };
    let prev_acc = previous.map(|s| format!("{:.2}", s.acc.unwrap_or(0.0))).unwrap_or_default();
    let prev_pp = previous.map(|s| format!("{:.0}", s.pp.unwrap_or(0.0))).unwrap_or_default();
    let prev_rank = rank_before.map(|r| r.to_string()).unwrap_or_default();

    let curr_score_str = score.score.to_string();
    let curr_acc = format!("{:.2}", score.acc.unwrap_or(0.0));
    let curr_pp = format!("{:.0}", score.pp.unwrap_or(0.0));
    let curr_rank = rank_after.map(|r| r.to_string()).unwrap_or_default();

    let sid = score.scoreid.unwrap_or(0);
    let online_score_id = -sid;

    let beatmap_chart = format!(
        "chartId:beatmap|chartUrl:https://osu.ppy.sh/b/{}|chartName:Local Beatmap Ranking|rankBefore:{}|rankAfter:{}|maxComboBefore:{}|maxComboAfter:{}|accuracyBefore:{}|accuracyAfter:{}|rankedScoreBefore:{}|rankedScoreAfter:{}|ppBefore:{}|ppAfter:{}|onlineScoreId:{}",
        bmap.beatmap_id,
        prev_rank, curr_rank,
        prev_max_combo, curr_max_combo,
        prev_acc, curr_acc,
        prev_score_str, curr_score_str,
        prev_pp, curr_pp,
        online_score_id,
    );

    let overall_chart = format!(
        "chartId:overall|chartUrl:http://127.0.0.1:5000/u/2|chartName:Overall Ranking|rankBefore:{}|rankAfter:{}|rankedScoreBefore:{}|rankedScoreAfter:{}|totalScoreBefore:{}|totalScoreAfter:{}|maxComboBefore:{}|maxComboAfter:{}|accuracyBefore:{:.2}|accuracyAfter:{:.2}|ppBefore:{:.0}|ppAfter:{:.0}|achievements-new:{}|onlineScoreId:{}",
        before.rank, after.rank,
        before.ranked_score, after.ranked_score,
        before.total_score, after.total_score,
        before.max_combo, after.max_combo,
        before.acc, after.acc,
        before.pp, after.pp,
        achievements_new.unwrap_or(""),
        online_score_id,
    );

    format!("{}\n{}\n{}", meta, beatmap_chart, overall_chart).into_bytes()
}

pub async fn process_native_submission(state: Arc<RwLock<AppState>>, sub: DecodedSubmission) -> Result<Vec<u8>, String> {
    let s = state.read().await;
    let player = s.player.as_ref().ok_or_else(|| "No player logged in".to_string())?;

    if sub.score.name != player.name {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Cannot submit another player's replay."));
        }
        return Err("Player mismatch".to_string());
    }

    {
        let db_conn = s.db.lock().await;
        if let Ok(Some(existing_id)) =
            db::check_duplicate_score(&db_conn, Some(&sub.submission_checksum), sub.score.replay_md5.as_deref(), sub.identity.as_deref())
        {
            let _ = db::increment_playcount(&db_conn, &player.name);
            let playcount = db::get_playcount(&db_conn, &player.name).unwrap_or(1);
            let bmap = db::get_beatmap_by_md5(&db_conn, &sub.score.md5).unwrap_or(None).unwrap_or_else(Beatmap::blank);
            let existing_score = db::get_score_by_id(&db_conn, existing_id).unwrap_or(None);
            let overall_max = db::get_max_combo_for_player(&db_conn, &player.name, sub.score.mode).unwrap_or(0);
            let stats = ProfileStats {
                rank: player.rank,
                pp: player.pp as f64,
                acc: player.acc,
                ranked_score: player.ranked_score,
                total_score: player.total_score,
                max_combo: overall_max,
            };
            let charts = build_charts(
                &bmap,
                existing_score.as_ref().unwrap_or(&sub.score),
                &stats,
                &stats,
                existing_score.as_ref(),
                Some(1),
                Some(1),
                Some(overall_max),
                playcount,
                None,
            );
            drop(db_conn);
            drop(s);
            let mut s_write = state.write().await;
            if let Some(ref mut p) = s_write.player {
                p.playcount = playcount;
                p.enqueue_stats();
            }
            return Ok(charts);
        }
    }

    if !(0..=3).contains(&sub.score.mode) {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Cannot submit: invalid game mode."));
        }
        return Err("Invalid game mode".to_string());
    }

    let score_mods = Mods::from_bits_truncate(sub.score.mods);
    if score_mods.intersects(s.invalid_mods) {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Cannot submit: invalid mods."));
        }
        return Err("Invalid mods".to_string());
    }

    if !sub.passed {
        let player_name = player.name.clone();
        let (playcount, overall_max, newly_unlocked_medals) = {
            let db_conn = s.db.lock().await;
            let _ = db::ensure_profile(&db_conn, &player_name);
            let _ = db::increment_playcount(&db_conn, &player_name);
            let pc = db::get_playcount(&db_conn, &player_name).unwrap_or(1);
            let overall_max = db::get_max_combo_for_player(&db_conn, &player_name, sub.score.mode).unwrap_or(0);

            let existing_medals: HashSet<i32> = db::get_user_medals(&db_conn, &player_name)
                .unwrap_or_default()
                .into_iter()
                .map(|m| m.medal_id)
                .collect();
            let bmap_ref = db::get_beatmap_by_md5(&db_conn, &sub.score.md5).unwrap_or(None).unwrap_or_else(Beatmap::blank);
            let mode_hits = db::get_mode_total_hits(&db_conn, &player_name, sub.score.mode).unwrap_or(0);
            let new_ids = medals::evaluate_score_submission(&sub.score, &bmap_ref, None, pc, mode_hits, &existing_medals, false);
            if !new_ids.is_empty() {
                let now = chrono::Utc::now().timestamp();
                let batch: Vec<(i32, i64)> = new_ids.iter().map(|&id| (id, now)).collect();
                let _ = db::add_user_medals_batch(&db_conn, &player_name, &batch);
            }
            (pc, overall_max, new_ids)
        };

        let bmap = {
            let db_conn = s.db.lock().await;
            db::get_beatmap_by_md5(&db_conn, &sub.score.md5).unwrap_or(None)
        };

        let (prev_rank_str, prev_combo_str, prev_acc_str, prev_score_str, prev_pp_str) = if let Some(ref b) = bmap {
            let db_conn = s.db.lock().await;
            let all_local = db::get_all_scores_on_map(&db_conn, &sub.score.md5, sub.score.mode).unwrap_or_default();
            drop(db_conn);

            let api_key = s.config.osu_api_key.as_deref();
            let pp_lb = s.config.pp_leaderboard;
            let (prev, r_before, _, map_max_before) = calculate_map_ranks(
                &s.http,
                api_key,
                b,
                sub.score.mode,
                s.mode,
                pp_lb,
                None,
                &all_local,
                &player_name,
                &sub.score,
            )
            .await;

            let prev_rank_str = r_before.map(|r| r.to_string()).unwrap_or_default();
            let effective_combo = map_max_before.or_else(|| prev.as_ref().map(|s| s.max_combo));
            let prev_combo_str = effective_combo.map(|c| c.to_string()).unwrap_or_default();
            let prev_acc_str = prev.as_ref().map(|s| format!("{:.2}", s.acc.unwrap_or(0.0))).unwrap_or_default();
            let prev_score_str = prev.as_ref().map(|s| s.score.to_string()).unwrap_or_default();
            let prev_pp_str = prev.as_ref().map(|s| format!("{:.0}", s.pp.unwrap_or(0.0))).unwrap_or_default();
            (prev_rank_str, prev_combo_str, prev_acc_str, prev_score_str, prev_pp_str)
        } else {
            (String::new(), String::new(), String::new(), String::new(), String::new())
        };

        let bmap_id = bmap.as_ref().map(|b| b.beatmap_id).unwrap_or(0);
        let bmapset_id = bmap.as_ref().map(|b| b.beatmapset_id).unwrap_or(0);

        let meta = format!(
            "beatmapId:{}|beatmapSetId:{}|beatmapPlaycount:{}|beatmapPasscount:{}|approvedDate:0",
            bmap_id, bmapset_id, playcount, playcount
        );

        let beatmap_chart = format!(
            "chartId:beatmap|chartUrl:https://osu.ppy.sh/b/{}|chartName:Local Beatmap Ranking|rankBefore:{}|rankAfter:{}|maxComboBefore:{}|maxComboAfter:{}|accuracyBefore:{}|accuracyAfter:{}|rankedScoreBefore:{}|rankedScoreAfter:{}|ppBefore:{}|ppAfter:{}|onlineScoreId:0",
            bmap_id,
            prev_rank_str, prev_rank_str,
            prev_combo_str, prev_combo_str,
            prev_acc_str, prev_acc_str,
            prev_score_str, prev_score_str,
            prev_pp_str, prev_pp_str,
        );

        let achievements_slugs: Vec<&str> = newly_unlocked_medals
            .iter()
            .filter_map(|&id| get_medal_by_id(id).map(|m| m.icon_url.as_str()))
            .collect();
        let achievements_str = if achievements_slugs.is_empty() {
            None
        } else {
            Some(achievements_slugs.join("/"))
        };

        let overall_chart = format!(
            "chartId:overall|chartUrl:http://127.0.0.1:5000/u/2|chartName:Overall Ranking|rankBefore:{}|rankAfter:{}|rankedScoreBefore:{}|rankedScoreAfter:{}|totalScoreBefore:{}|totalScoreAfter:{}|maxComboBefore:{}|maxComboAfter:{}|accuracyBefore:{:.2}|accuracyAfter:{:.2}|ppBefore:{:.0}|ppAfter:{:.0}|achievements-new:{}|onlineScoreId:0",
            player.rank, player.rank,
            player.ranked_score, player.ranked_score,
            player.total_score, player.total_score,
            overall_max, overall_max,
            player.acc, player.acc,
            player.pp, player.pp,
            achievements_str.as_deref().unwrap_or(""),
        );

        let charts = format!("{}\n{}\n{}", meta, beatmap_chart, overall_chart).into_bytes();

        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.playcount = playcount;
            for &id in &newly_unlocked_medals {
                if let Some(m) = get_medal_by_id(id) {
                    let chat_announcement = format!("{} unlocked the medal: [{}] — {}", p.name, m.name, m.description);
                    p.queue.extend_from_slice(&packets::local_message(&chat_announcement, "#osu"));
                }
            }
            p.enqueue_stats();
        }

        return Ok(charts);
    }

    let bmap = {
        let db_conn = s.db.lock().await;
        let local = db::get_beatmap_by_md5(&db_conn, &sub.score.md5).unwrap_or(None);
        if local.is_some() {
            local
        } else {
            drop(db_conn);
            let mut fetched = None;
            if let Some(ref api_key) = s.config.osu_api_key {
                fetched = utils::fetch_beatmap_from_api(&s.http, api_key, &[("h", sub.score.md5.clone())]).await;
            }
            if fetched.is_none() {
                if let Some(songs_dir) = utils::resolve_songs_folder(&s.config) {
                    if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(&songs_dir, None, None, Some(&sub.score.md5), None) {
                        let db_conn = s.db.lock().await;
                        let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
                        fetched = Some(local_bmap);
                    }
                }
            }
            if fetched.is_none() {
                let player_match = s.player.as_ref().and_then(|p| {
                    if p.map_md5 == sub.score.md5 {
                        Some((p.map_id, p.info_text.clone()))
                    } else {
                        None
                    }
                });
                if let Some((mid, _info)) = player_match {
                    if mid > 0 {
                        let db_conn = s.db.lock().await;
                        if let Ok(Some(mut b)) = db::get_beatmap_by_id(&db_conn, mid as i64) {
                            if !b.version.to_lowercase().contains("practice") {
                                b.file_md5 = sub.score.md5.clone();
                                fetched = Some(b);
                            }
                        }
                    }
                }
            }
            if fetched.is_none() {
                if let Some(ref np) = s.last_np_map {
                    if np.file_md5 == sub.score.md5 {
                        fetched = Some(np.clone());
                    }
                }
            }
            if let Some(ref bmap) = fetched {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, bmap);
            }
            fetched
        }
    };

    let mut bmap = bmap;
    if let Some(ref mut b) = bmap {
        if b.beatmap_id > 0 {
            let db_conn = s.db.lock().await;
            if let Ok(Some(canonical)) = db::get_beatmap_by_id(&db_conn, b.beatmap_id) {
                // If canonical entry has a ranked status and a different MD5, this is an altered/cloned diff
                if canonical.approved > 0 && !canonical.file_md5.is_empty() && !canonical.file_md5.eq_ignore_ascii_case(&b.file_md5) && canonical.file_md5 != "old_draft_md5" {
                    b.beatmap_id = 0;
                    b.approved = 0;
                }
            }
        }
    }

    let Some(bmap) = bmap else {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Beatmap must exist on Bancho to submit."));
        }
        return Err("Map not found on bancho".to_string());
    };

    let leaderboard_worthy = [1, 2, 3, 4];
    if !leaderboard_worthy.contains(&bmap.approved) {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Cannot submit unranked beatmap."));
        }
        return Err("Unranked map".to_string());
    }

    let mut score = sub.score;

    let file_content = utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, &bmap).await;

    let Some(content) = file_content else {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Failed to retrieve beatmap file (.osu)."));
        }
        return Err("Missing .osu file".to_string());
    };

    let parsed_map = rosu_pp::Beatmap::from_bytes(content.as_bytes()).ok();
    if let Some(ref map) = parsed_map {
        let game_mode = match score.mode {
            0 => rosu_pp::model::mode::GameMode::Osu,
            1 => rosu_pp::model::mode::GameMode::Taiko,
            2 => rosu_pp::model::mode::GameMode::Catch,
            3 => rosu_pp::model::mode::GameMode::Mania,
            _ => rosu_pp::model::mode::GameMode::Osu,
        };

        let result = rosu_pp::Performance::new(map)
            .mode_or_ignore(game_mode)
            .mods(score.mods)
            .n300(score.n300 as u32)
            .n100(score.n100 as u32)
            .n50(score.n50 as u32)
            .n_katu(score.nkatu as u32)
            .n_geki(score.ngeki as u32)
            .misses(score.nmiss as u32)
            .combo(score.max_combo as u32)
            .calculate();

        score.pp = Some(result.pp());
        let acc = utils::calculate_accuracy(score.mode as u8, score.n300, score.n100, score.n50, score.ngeki, score.nkatu, score.nmiss);
        score.acc = Some(acc);
    }

    score.mods_str = Some(Mods::from_bits_truncate(score.mods).short_name());

    let player_name = player.name.clone();
    let mode = s.mode;
    let ping_recent = s.config.ping_user_when_recent_score;
    let enable_recent = s.config.enable_recent_channel;

    let (previous, rank_before, rank_after, max_combo_before, overall_max_before) = {
        let db_conn = s.db.lock().await;
        let all_local = db::get_all_scores_on_map(&db_conn, &score.md5, score.mode).unwrap_or_default();
        let overall_max = db::get_max_combo_for_player(&db_conn, &player_name, score.mode).unwrap_or(0);
        drop(db_conn);

        let api_key = s.config.osu_api_key.as_deref();
        let pp_lb = s.config.pp_leaderboard;
        let (prev, r_before, r_after, map_max_before) = calculate_map_ranks(
            &s.http,
            api_key,
            &bmap,
            score.mode,
            mode,
            pp_lb,
            parsed_map.as_ref(),
            &all_local,
            &player_name,
            &score,
        )
        .await;
        (prev, r_before, r_after, map_max_before, overall_max)
    };

    let before_stats = ProfileStats {
        rank: player.rank,
        pp: player.pp as f64,
        acc: player.acc,
        ranked_score: player.ranked_score,
        total_score: player.total_score,
        max_combo: overall_max_before,
    };

    let bmap_status = leaderboard::status_to_db_key(bmap.approved);
    let (mut newly_unlocked_medals, existing_medals) = {
        let db_conn = s.db.lock().await;
        let _ = db::increment_playcount(&db_conn, &player_name);
        match db::insert_score(&db_conn, &score, bmap_status) {
            Ok(id) => {
                score.scoreid = Some(id);
            }
            Err(e) => {
                utils::log_error(&format!("Failed to insert score: {}", e));
                return Err("Database insertion failed".to_string());
            }
        }

        let existing_medals: HashSet<i32> = db::get_user_medals(&db_conn, &player_name)
            .unwrap_or_default()
            .into_iter()
            .map(|m| m.medal_id)
            .collect();
        let pc = db::get_playcount(&db_conn, &player_name).unwrap_or(1);
        let mode_hits = db::get_mode_total_hits(&db_conn, &player_name, score.mode).unwrap_or(0);
        let new_ids = medals::evaluate_score_submission(&score, &bmap, parsed_map.as_ref(), pc, mode_hits, &existing_medals, true);
        if !new_ids.is_empty() {
            let now = chrono::Utc::now().timestamp();
            let batch: Vec<(i32, i64)> = new_ids.iter().map(|&id| (id, now)).collect();
            let _ = db::add_user_medals_batch(&db_conn, &player_name, &batch);
        }
        (new_ids, existing_medals)
    };
    drop(s);

    let (after_stats, charts, http, discord_webhook_url, discord_webhook_min_pp) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, &player_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, &player_name).unwrap_or(1);
        let daily_key = s.config.osu_daily_api_key.clone();
        let http = s.http.clone();
        let discord_webhook_url = s.config.discord_webhook_url.clone();
        let discord_webhook_min_pp = s.config.discord_webhook_min_pp;
        let db_arc = s.db.clone();
        drop(db_conn);
        drop(s);

        let mut s_write = state.write().await;
        let player = s_write.player.as_mut().unwrap();
        player.mode = score.mode as u8;
        player.calculate_stats(&scores, mode, playcount);
        let pp = player.pp;
        let p_mode = player.mode;
        drop(s_write);

        let rank = if let Some(ref api_key) = daily_key { utils::get_rank_from_daily(&http, api_key, pp, p_mode).await } else { None };

        let mut s_write = state.write().await;
        let player = s_write.player.as_mut().unwrap();
        if let Some(r) = rank {
            player.rank = r;
            let rank_ids = medals::evaluate_rank_medals(r, &existing_medals);
            for id in rank_ids {
                if !newly_unlocked_medals.contains(&id) {
                    newly_unlocked_medals.push(id);
                }
            }
        }

        if !newly_unlocked_medals.is_empty() {
            let conn = db_arc.lock().await;
            let now = chrono::Utc::now().timestamp();
            let batch: Vec<(i32, i64)> = newly_unlocked_medals.iter().map(|&id| (id, now)).collect();
            let _ = db::add_user_medals_batch(&conn, &player_name, &batch);
        }

        let after_stats = ProfileStats {
            rank: player.rank,
            pp: player.pp as f64,
            acc: player.acc,
            ranked_score: player.ranked_score,
            total_score: player.total_score,
            max_combo: overall_max_before.max(score.max_combo),
        };

        let achievements_slugs: Vec<&str> = newly_unlocked_medals
            .iter()
            .filter_map(|&id| get_medal_by_id(id).map(|m| m.icon_url.as_str()))
            .collect();
        let achievements_str = if achievements_slugs.is_empty() {
            None
        } else {
            Some(achievements_slugs.join("/"))
        };

        let charts = build_charts(
            &bmap,
            &score,
            &before_stats,
            &after_stats,
            previous.as_ref(),
            rank_before,
            rank_after,
            max_combo_before,
            playcount,
            achievements_str.as_deref(),
        );

        let grade =
            utils::get_grade(score.mode as u8, score.n300, score.n100, score.n50, score.ngeki, score.nkatu, score.nmiss, score.mods, score.acc.unwrap_or(0.0));
        let mods_str = score.mods_str.as_deref().unwrap_or("NM");
        let mode_prefix = if score.mode != 0 { format!("[{}] ", utils::get_mode_name(score.mode as u8)) } else { String::new() };

        let pp = score.pp.unwrap_or(0.0);
        let notif = if pp > 0.0 {
            format!("Score submitted! ({:.0}pp)", pp)
        } else {
            "Score submitted!".to_string()
        };
        player.queue.extend_from_slice(&packets::notification(&notif));

        for &id in &newly_unlocked_medals {
            if let Some(m) = get_medal_by_id(id) {
                let chat_announcement = format!("{} unlocked the medal: [{}] — {}", player.name, m.name, m.description);
                player.queue.extend_from_slice(&packets::local_message(&chat_announcement, "#osu"));
                if enable_recent {
                    player.queue.extend_from_slice(&packets::local_message(&chat_announcement, "#recent"));
                }
            }
        }

        if enable_recent {
            let msg = format!(
                "{}{} - {} [{}]\n+{} {:.2}% {} {:.0}PP {}x/{}x {}X",
                mode_prefix,
                bmap.artist,
                bmap.title,
                bmap.version,
                mods_str,
                score.acc.unwrap_or(0.0),
                grade,
                score.pp.unwrap_or(0.0),
                score.max_combo,
                bmap.max_combo,
                score.nmiss,
            );
            let mut full_msg = msg;
            if ping_recent {
                full_msg += &format!("\nachieved by {}", player.name);
            }
            player.queue.extend_from_slice(&packets::local_message(&full_msg, "#recent"));
        }
        player.enqueue_stats();

        (after_stats, charts, http, discord_webhook_url, discord_webhook_min_pp)
    };

    if let Some(webhook_url) = discord_webhook_url {
        let meets_pp = match discord_webhook_min_pp {
            Some(min_pp) => score.pp.unwrap_or(0.0) >= min_pp,
            None => true,
        };

        if meets_pp {
            let player_rank = after_stats.rank;
            let player_pp = after_stats.pp;
            crate::handlers::webhook::send_score_webhook(
                http,
                webhook_url,
                player_name.clone(),
                score,
                bmap,
                rank_after,
                player_rank,
                player_pp,
            );
        }
    }

    utils::log_success(&format!("{} has successfully submitted a score!", player_name));
    Ok(charts)
}

pub async fn score_submit(state: Arc<RwLock<AppState>>, score: Score, replay: &Replay) {
    let sub = DecodedSubmission {
        score,
        submission_checksum: String::new(),
        passed: !replay.is_failed(),
        date: String::new(),
        osu_version: String::new(),
        replay_frames: Some(replay.raw_frames.clone()),
        identity: None,
    };

    let _ = process_native_submission(state, sub).await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_charts_format() {
        let mut bmap = Beatmap::blank();
        bmap.beatmap_id = 123;
        bmap.beatmapset_id = 456;
        bmap.approved = 1;
        bmap.artist = "Artist".to_string();
        bmap.title = "Title".to_string();
        bmap.version = "Insane".to_string();
        bmap.file_md5 = "abc".to_string();
        bmap.max_combo = 500;
        bmap.difficultyrating = 5.0;

        let score = Score {
            scoreid: Some(10),
            md5: "abc".to_string(),
            name: "Alice".to_string(),
            score: 1000000,
            max_combo: 500,
            mods: 0,
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 50,
            nkatu: 0,
            nmiss: 0,
            time: 123456,
            perfect: true,
            pp: Some(150.0),
            acc: Some(100.0),
            mode: 0,
            mods_str: Some("NM".to_string()),
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };

        let before = ProfileStats { rank: 100, pp: 1000.0, acc: 98.5, ranked_score: 5000000, total_score: 10000000, max_combo: 450 };

        let after = ProfileStats { rank: 95, pp: 1050.0, acc: 98.7, ranked_score: 6000000, total_score: 11000000, max_combo: 500 };

        let charts_bytes = build_charts(&bmap, &score, &before, &after, None, None, Some(1), None, 5, None);
        let charts_str = String::from_utf8(charts_bytes).unwrap();

        assert!(charts_str.contains("beatmapId:123|beatmapSetId:456|beatmapPlaycount:5|beatmapPasscount:5"));
        assert!(charts_str.contains("chartId:beatmap|chartUrl:https://osu.ppy.sh/b/123|chartName:Local Beatmap Ranking"));
        assert!(charts_str.contains("rankAfter:1"));
        assert!(charts_str.contains("onlineScoreId:-10"));
        assert!(charts_str.contains("chartId:overall"));
        assert!(charts_str.contains("rankBefore:100|rankAfter:95"));
        assert!(charts_str.contains("ppBefore:1000|ppAfter:1050"));

        // When rank_after is None (outside top 50), rankAfter should be empty
        let charts_outside = build_charts(&bmap, &score, &before, &after, None, None, None, None, 5, None);
        let str_outside = String::from_utf8(charts_outside).unwrap();
        assert!(str_outside.contains("rankBefore:|rankAfter:|maxComboBefore:"));

        // When placing #5 with previous PB at #8
        let charts_p5 = build_charts(&bmap, &score, &before, &after, Some(&score), Some(8), Some(5), Some(500), 5, None);
        let str_p5 = String::from_utf8(charts_p5).unwrap();
        assert!(str_p5.contains("rankBefore:8|rankAfter:5|maxComboBefore:"));

        // Test combo not showing NEW when higher was achieved in a different submitted score
        let mut lower_combo_score = score.clone();
        lower_combo_score.max_combo = 300;
        let chart_bytes_existing = build_charts(&bmap, &lower_combo_score, &before, &after, Some(&score), Some(1), Some(1), Some(800), 5, None);
        let chart_str_existing = String::from_utf8(chart_bytes_existing).unwrap();
        // maxComboBefore and maxComboAfter both retain 800 -> no NEW combo
        assert!(chart_str_existing.contains("maxComboBefore:800|maxComboAfter:800"));

        // When beating previous max combo (e.g. 950 > 800)
        let mut higher_combo_score = score.clone();
        higher_combo_score.max_combo = 950;
        let chart_bytes_new = build_charts(&bmap, &higher_combo_score, &before, &after, Some(&score), Some(1), Some(1), Some(800), 5, None);
        let chart_str_new = String::from_utf8(chart_bytes_new).unwrap();
        assert!(chart_str_new.contains("maxComboBefore:800|maxComboAfter:950"));

        // Test achievements-new string populated in overall chart
        let charts_medals = build_charts(&bmap, &score, &before, &after, None, None, Some(1), None, 5, Some("osu-combo-500/osu-skill-pass-1"));
        let str_medals = String::from_utf8(charts_medals).unwrap();
        assert!(str_medals.contains("achievements-new:osu-combo-500/osu-skill-pass-1|onlineScoreId:-10"));
    }

    #[tokio::test]
    async fn test_calculate_map_ranks_various_positions() {
        let http = reqwest::Client::new();
        let bmap = Beatmap::blank();

        let make_score = |name: &str, score_val: i64, pp_val: f64| Score {
            scoreid: Some(1),
            md5: "abc".to_string(),
            name: name.to_string(),
            score: score_val,
            max_combo: 100,
            mods: 0,
            n300: 100,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            time: 0,
            perfect: true,
            pp: Some(pp_val),
            acc: Some(100.0),
            mode: 0,
            mods_str: None,
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };

        // 1. Solo play, no competitors in DB, no API key -> rank 1
        let new_score = make_score("Alice", 100_000, 50.0);
        let (prev, r_before, r_after, max_c_before) = calculate_map_ranks(
            &http,
            None,
            &bmap,
            0,
            None,
            false,
            None,
            &[],
            "Alice",
            &new_score,
        ).await;
        assert!(prev.is_none());
        assert_eq!(r_before, None);
        assert_eq!(r_after, Some(1));
        assert_eq!(max_c_before, None);

        // 2. Competitors exist locally: 4 players with higher scores -> rank 5
        let local_scores = vec![
            make_score("P1", 500_000, 200.0),
            make_score("P2", 400_000, 180.0),
            make_score("P3", 300_000, 150.0),
            make_score("P4", 200_000, 120.0),
            make_score("P5", 50_000, 30.0),
        ];
        let (prev, r_before, r_after, _) = calculate_map_ranks(
            &http,
            None,
            &bmap,
            0,
            None,
            false,
            None,
            &local_scores,
            "Alice",
            &new_score,
        ).await;
        assert!(prev.is_none());
        assert_eq!(r_before, None);
        assert_eq!(r_after, Some(5));

        // 3. 50 competitors better -> outside top 50 (rank_after: None)
        let mut top50: Vec<Score> = Vec::new();
        for i in 1..=50 {
            top50.push(make_score(&format!("Player{}", i), 1_000_000 - i * 1000, 100.0));
        }
        let low_score = make_score("Alice", 50_000, 10.0);
        let (prev, r_before, r_after, _) = calculate_map_ranks(
            &http,
            None,
            &bmap,
            0,
            None,
            false,
            None,
            &top50,
            "Alice",
            &low_score,
        ).await;
        assert!(prev.is_none());
        assert_eq!(r_before, None);
        assert_eq!(r_after, None);

        // 4. Player had previous score at rank 5, new score improves to rank 2
        let mut with_prev = local_scores.clone();
        with_prev.push(make_score("Alice", 80_000, 40.0)); // Alice previous was 5th (behind P1..P4)
        let improved_score = make_score("Alice", 450_000, 190.0); // beats P2..P5, behind P1 -> rank 2
        let (prev, r_before, r_after, _) = calculate_map_ranks(
            &http,
            None,
            &bmap,
            0,
            None,
            false,
            None,
            &with_prev,
            "Alice",
            &improved_score,
        ).await;
        assert!(prev.is_some());
        assert_eq!(r_before, Some(5));
        assert_eq!(r_after, Some(2));

        // 5. Player has multiple scores: Score A has combo 800 (low score), Score B has combo 200 (high score)
        // calculate_map_ranks must return max_combo_before = Some(800) even though previous is Score B
        let mut multi_scores = Vec::new();
        let mut score_a = make_score("Alice", 300_000, 100.0);
        score_a.max_combo = 800;
        let mut score_b = make_score("Alice", 800_000, 250.0);
        score_b.max_combo = 200;
        multi_scores.push(score_a);
        multi_scores.push(score_b);

        let mut score_c = make_score("Alice", 500_000, 150.0);
        score_c.max_combo = 400;

        let (prev, _, _, max_c_before) = calculate_map_ranks(
            &http,
            None,
            &bmap,
            0,
            None,
            false,
            None,
            &multi_scores,
            "Alice",
            &score_c,
        ).await;
        assert_eq!(prev.as_ref().map(|s| s.max_combo), Some(200));
        assert_eq!(max_c_before, Some(800));
    }

    #[test]
    fn test_rosu_pp_all_modes() {
        let osu_content = "osu file format v14\n\n[General]\nAudioFilename: a.mp3\nMode: 0\n\n[Metadata]\nTitle:T\nArtist:A\nCreator:C\nVersion:V\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n\n[TimingPoints]\n0,500,4,2,0,50,1,0\n\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n256,192,2000,1,0,0:0:0:0:\n256,192,3000,1,0,0:0:0:0:\n256,192,4000,1,0,0:0:0:0:\n";

        let parsed_map = rosu_pp::Beatmap::from_bytes(osu_content.as_bytes()).unwrap();

        for mode_val in 0..=3 {
            let game_mode = match mode_val {
                0 => rosu_pp::model::mode::GameMode::Osu,
                1 => rosu_pp::model::mode::GameMode::Taiko,
                2 => rosu_pp::model::mode::GameMode::Catch,
                3 => rosu_pp::model::mode::GameMode::Mania,
                _ => unreachable!(),
            };

            let result = rosu_pp::Performance::new(&parsed_map).mode_or_ignore(game_mode).combo(4).n300(4).calculate();

            assert!(result.pp() >= 0.0);
        }
    }

    #[tokio::test]
    async fn test_failed_play_does_not_queue_notification() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "TestPlayer").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let player = crate::types::player::Player::new("TestPlayer".to_string());
        app_state.player = Some(player);
        let shared_state = Arc::new(RwLock::new(app_state));

        let sub_score = Score {
            scoreid: None,
            md5: "abc".to_string(),
            name: "TestPlayer".to_string(),
            score: 1000,
            max_combo: 10,
            mods: 0,
            n300: 10,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 1,
            time: 12345,
            perfect: false,
            pp: None,
            acc: None,
            mode: 0,
            mods_str: None,
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let sub = DecodedSubmission {
            score: sub_score,
            submission_checksum: String::new(),
            passed: false,
            date: String::new(),
            osu_version: String::new(),
            replay_frames: None,
            identity: None,
        };

        let result = process_native_submission(shared_state.clone(), sub).await;
        assert!(result.is_ok(), "Failed play must return Ok(charts) to prevent client getting stuck on submitting score");
        let charts_str = String::from_utf8(result.unwrap()).unwrap();
        assert!(charts_str.contains("chartId:beatmap"));
        assert!(charts_str.contains("chartId:overall"));

        let s = shared_state.read().await;
        let p = s.player.as_ref().unwrap();
        assert_eq!(p.playcount, 1, "Failed play must increment playcount");

        let pkts = packets::split_packets(&p.queue);
        assert!(!pkts.iter().any(|pkt| pkt.id == packets::PacketId::ChoNotification as u16), "Failed plays must NOT send notification popups to the player");
        assert!(pkts.iter().any(|pkt| pkt.id == packets::PacketId::ChoUserStats as u16), "Failed plays must enqueue ChoUserStats to update profile panel immediately");

        let db_conn = s.db.lock().await;
        let scores = db::get_all_scores_on_map(&db_conn, "abc", 0).unwrap();
        assert!(scores.is_empty(), "Failed plays must NOT be inserted into database scores");
    }

    #[tokio::test]
    async fn test_score_submit_resolves_fallback_from_player_state() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "MyAngelKyrie").unwrap();

        let osu_content = "osu file format v14\n\n[General]\nAudioFilename: a.mp3\nMode: 0\n\n[Metadata]\nTitle:Kyrie Map\nArtist:Kyrie\nCreator:Mapper\nVersion:Insane\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n\n[TimingPoints]\n0,500,4,2,0,50,1,0\n\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n";

        let mut bmap = Beatmap::blank();
        bmap.beatmap_id = 5315861;
        bmap.approved = 1; // Ranked
        bmap.file_content = Some(osu_content.to_string());
        // In DB, the map might have been inserted with an empty or draft MD5
        bmap.file_md5 = "old_draft_md5".to_string();
        db::insert_beatmap(&conn, &bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("MyAngelKyrie".to_string());
        player.map_id = 5315861;
        player.map_md5 = "e10adc3949ba59abbe56e057f20f883e".to_string();
        app_state.player = Some(player);
        let shared_state = Arc::new(RwLock::new(app_state));

        let sub_score = Score {
            scoreid: None,
            md5: "e10adc3949ba59abbe56e057f20f883e".to_string(),
            name: "MyAngelKyrie".to_string(),
            score: 100000,
            max_combo: 1,
            mods: 0,
            n300: 1,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            time: 12345,
            perfect: true,
            pp: None,
            acc: Some(100.0),
            mode: 0,
            mods_str: None,
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let sub = DecodedSubmission {
            score: sub_score,
            submission_checksum: String::new(),
            passed: true,
            date: String::new(),
            osu_version: String::new(),
            replay_frames: None,
            identity: None,
        };

        let result = process_native_submission(shared_state.clone(), sub).await;
        assert!(result.is_ok(), "Score submission must succeed with player state fallback: {:?}", result.err());

        // Verify beatmap is now indexed by the score's MD5 in the DB
        let s = shared_state.read().await;
        let db_conn = s.db.lock().await;
        let found = db::get_beatmap_by_md5(&db_conn, "e10adc3949ba59abbe56e057f20f883e").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().beatmap_id, 5315861);
    }

    #[tokio::test]
    async fn test_player_mismatch_submission_does_not_deadlock() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut app_state = AppState::new(conn, crate::types::config::Config::default());
        app_state.player = Some(crate::types::player::Player::new("ActivePlayer".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let sub_score = Score {
            scoreid: None,
            md5: "dummy_md5".to_string(),
            name: "OtherPlayer".to_string(),
            score: 1000,
            max_combo: 10,
            mods: 0,
            n300: 10,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            time: 12345,
            perfect: true,
            pp: None,
            acc: Some(100.0),
            mode: 0,
            mods_str: None,
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let sub = DecodedSubmission {
            score: sub_score,
            submission_checksum: String::new(),
            passed: true,
            date: String::new(),
            osu_version: String::new(),
            replay_frames: None,
            identity: None,
        };

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(500),
            process_native_submission(shared_state.clone(), sub),
        )
        .await
        .expect("Submission must not deadlock on player mismatch");

        assert_eq!(result.err(), Some("Player mismatch".to_string()));
        let s = shared_state.read().await;
        let p = s.player.as_ref().unwrap();
        assert!(!p.queue.is_empty(), "Notification must be queued for mismatch");
    }

    #[tokio::test]
    async fn test_score_submit_rejects_altered_diff_with_stolen_beatmap_id() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "TestPlayer").unwrap();

        let mut canonical_bmap = Beatmap::blank();
        canonical_bmap.beatmap_id = 99999;
        canonical_bmap.approved = 1;
        canonical_bmap.file_md5 = "canonical_md5_hash".to_string();
        db::insert_beatmap(&conn, &canonical_bmap).unwrap();

        let mut altered_bmap = Beatmap::blank();
        altered_bmap.beatmap_id = 99999;
        altered_bmap.approved = 1;
        altered_bmap.file_md5 = "altered_custom_diff_md5".to_string();
        db::insert_beatmap(&conn, &altered_bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("TestPlayer".to_string());
        player.map_id = 99999;
        player.map_md5 = "altered_custom_diff_md5".to_string();
        app_state.player = Some(player);
        let shared_state = Arc::new(RwLock::new(app_state));

        let sub_score = Score {
            scoreid: None,
            md5: "altered_custom_diff_md5".to_string(),
            name: "TestPlayer".to_string(),
            score: 100000,
            max_combo: 100,
            mods: 0,
            n300: 50,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            time: 12345,
            perfect: true,
            pp: None,
            acc: Some(100.0),
            mode: 0,
            mods_str: None,
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let sub = DecodedSubmission {
            score: sub_score,
            submission_checksum: String::new(),
            passed: true,
            date: String::new(),
            osu_version: String::new(),
            replay_frames: None,
            identity: None,
        };

        let result = process_native_submission(shared_state.clone(), sub).await;
        assert_eq!(result.err(), Some("Unranked map".to_string()));
    }

    #[tokio::test]
    async fn test_score_submit_awards_medals_and_includes_in_charts() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Kyrie").unwrap();

        let osu_content = "osu file format v14\n\n[General]\nAudioFilename: a.mp3\nMode: 0\n\n[Metadata]\nTitle:Medal Test\nArtist:Kyrie\nCreator:Mapper\nVersion:Insane\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:8\nApproachRate:9\nSliderMultiplier:1.4\nSliderTickRate:1\n\n[TimingPoints]\n0,200,4,2,0,50,1,0\n\n[HitObjects]\n0,0,100,1,0,0:0:0:0:\n512,384,200,1,0,0:0:0:0:\n0,384,300,1,0,0:0:0:0:\n512,0,400,1,0,0:0:0:0:\n0,0,500,1,0,0:0:0:0:\n512,384,600,1,0,0:0:0:0:\n0,384,700,1,0,0:0:0:0:\n512,0,800,1,0,0:0:0:0:\n";

        let mut canonical_bmap = Beatmap::blank();
        canonical_bmap.beatmap_id = 12345;
        canonical_bmap.approved = 1;
        canonical_bmap.file_md5 = "medals_test_md5".to_string();
        canonical_bmap.difficultyrating = 3.5;
        canonical_bmap.max_combo = 500;
        canonical_bmap.file_content = Some(osu_content.to_string());
        db::insert_beatmap(&conn, &canonical_bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("Kyrie".to_string());
        player.map_id = 12345;
        player.map_md5 = "medals_test_md5".to_string();
        app_state.player = Some(player);
        let shared_state = Arc::new(RwLock::new(app_state));

        let sub_score = Score {
            scoreid: None,
            md5: "medals_test_md5".to_string(),
            name: "Kyrie".to_string(),
            score: 5_000_000,
            max_combo: 500,
            mods: 0,
            n300: 450,
            n100: 50,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            time: 12345,
            perfect: true,
            pp: Some(150.0),
            acc: Some(98.5),
            mode: 0,
            mods_str: None,
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let sub = DecodedSubmission {
            score: sub_score,
            submission_checksum: String::new(),
            passed: true,
            date: String::new(),
            osu_version: String::new(),
            replay_frames: None,
            identity: None,
        };

        let result = process_native_submission(shared_state.clone(), sub).await;
        assert!(result.is_ok());
        let charts_str = String::from_utf8(result.unwrap()).unwrap();

        // Check that achievements-new is present with expected slugs
        assert!(charts_str.contains("achievements-new:"));
        // 500 combo is osu-combo-500
        assert!(charts_str.contains("osu-combo-500"));
        // 1★, 2★, 3★ passes: osu-skill-pass-1, 2, 3
        assert!(charts_str.contains("osu-skill-pass-1"));
        assert!(charts_str.contains("osu-skill-pass-3"));
        // 1★, 2★, 3★ FCs: osu-skill-fc-1, 2, 3
        assert!(charts_str.contains("osu-skill-fc-1"));
        assert!(charts_str.contains("osu-skill-fc-3"));

        // Verify database persistence
        let s = shared_state.read().await;
        let db_conn = s.db.lock().await;
        let medals = db::get_user_medals(&db_conn, "Kyrie").unwrap();
        assert!(!medals.is_empty());
        let medal_ids: Vec<i32> = medals.iter().map(|m| m.medal_id).collect();
        assert!(medal_ids.contains(&1)); // 500 combo
        assert!(medal_ids.contains(&55)); // 1 star pass
        assert!(medal_ids.contains(&57)); // 3 star pass
        assert!(medal_ids.contains(&63)); // 1 star FC
        assert!(medal_ids.contains(&65)); // 3 star FC

        // Verify in-game notifications and chat announcements queued
        let p = s.player.as_ref().unwrap();
        let pkts = packets::split_packets(&p.queue);
        let notif_count = pkts.iter().filter(|pkt| pkt.id == packets::PacketId::ChoNotification as u16).count();
        // 1 score submit notification (toast)
        assert_eq!(notif_count, 1);
        let msg_count = pkts.iter().filter(|pkt| pkt.id == packets::PacketId::ChoSendMessage as u16).count();
        // Announcements in #osu for each unlocked medal
        assert!(msg_count >= 5);
    }
}

