use crate::db;
use crate::handlers::score_decode::DecodedSubmission;
use crate::packets;
use crate::state::AppState;
use crate::types::beatmap::Beatmap;
use crate::types::leaderboard;
use crate::types::mods::Mods;
use crate::types::replay::Replay;
use crate::types::score::Score;
use crate::utils;
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
        "chartId:overall|chartUrl:http://127.0.0.1:5000/u/2|chartName:Overall Ranking|rankBefore:{}|rankAfter:{}|rankedScoreBefore:{}|rankedScoreAfter:{}|totalScoreBefore:{}|totalScoreAfter:{}|maxComboBefore:{}|maxComboAfter:{}|accuracyBefore:{:.2}|accuracyAfter:{:.2}|ppBefore:{:.0}|ppAfter:{:.0}|achievements-new:|onlineScoreId:{}",
        before.rank, after.rank,
        before.ranked_score, after.ranked_score,
        before.total_score, after.total_score,
        before.max_combo, after.max_combo,
        before.acc, after.acc,
        before.pp, after.pp,
        online_score_id,
    );

    format!("{}\n{}\n{}", meta, beatmap_chart, overall_chart).into_bytes()
}

pub async fn process_native_submission(state: Arc<RwLock<AppState>>, sub: DecodedSubmission) -> Result<Vec<u8>, String> {
    let s = state.read().await;
    let player = s.player.as_ref().ok_or_else(|| "No player logged in".to_string())?;

    if sub.score.name != player.name {
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
            let bmap = db::get_beatmap_by_md5(&db_conn, &sub.score.md5).unwrap_or(None);
            if let Some(bmap) = bmap {
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
                );
                return Ok(charts);
            }
        }
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
        return Err("Failed play".to_string());
    }

    if !(0..=3).contains(&sub.score.mode) {
        drop(s);
        let mut s_write = state.write().await;
        if let Some(ref mut p) = s_write.player {
            p.queue.extend_from_slice(&packets::notification("Cannot submit: invalid game mode."));
        }
        return Err("Invalid game mode".to_string());
    }

    let bmap = {
        let db_conn = s.db.lock().await;
        let local = db::get_beatmap_by_md5(&db_conn, &sub.score.md5).unwrap_or(None);
        if local.is_some() {
            local
        } else if let Some(ref api_key) = s.config.osu_api_key {
            drop(db_conn);
            let fetched = utils::fetch_beatmap_from_api(&s.http, api_key, &[("h", sub.score.md5.clone())]).await;
            if let Some(ref bmap) = fetched {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, bmap);
            }
            fetched
        } else {
            None
        }
    };

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
    {
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
    }
    drop(s);

    let (_after_stats, charts) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, &player_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, &player_name).unwrap_or(1);
        let daily_key = s.config.osu_daily_api_key.clone();
        let http = s.http.clone();
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
        }

        let after_stats = ProfileStats {
            rank: player.rank,
            pp: player.pp as f64,
            acc: player.acc,
            ranked_score: player.ranked_score,
            total_score: player.total_score,
            max_combo: overall_max_before.max(score.max_combo),
        };

        let charts = build_charts(&bmap, &score, &before_stats, &after_stats, previous.as_ref(), rank_before, rank_after, max_combo_before, playcount);

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

        (after_stats, charts)
    };

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

        let charts_bytes = build_charts(&bmap, &score, &before, &after, None, None, Some(1), None, 5);
        let charts_str = String::from_utf8(charts_bytes).unwrap();

        assert!(charts_str.contains("beatmapId:123|beatmapSetId:456|beatmapPlaycount:5|beatmapPasscount:5"));
        assert!(charts_str.contains("chartId:beatmap|chartUrl:https://osu.ppy.sh/b/123|chartName:Local Beatmap Ranking"));
        assert!(charts_str.contains("rankAfter:1"));
        assert!(charts_str.contains("onlineScoreId:-10"));
        assert!(charts_str.contains("chartId:overall"));
        assert!(charts_str.contains("rankBefore:100|rankAfter:95"));
        assert!(charts_str.contains("ppBefore:1000|ppAfter:1050"));

        // When rank_after is None (outside top 50), rankAfter should be empty
        let charts_outside = build_charts(&bmap, &score, &before, &after, None, None, None, None, 5);
        let str_outside = String::from_utf8(charts_outside).unwrap();
        assert!(str_outside.contains("rankBefore:|rankAfter:|maxComboBefore:"));

        // When placing #5 with previous PB at #8
        let charts_p5 = build_charts(&bmap, &score, &before, &after, Some(&score), Some(8), Some(5), Some(500), 5);
        let str_p5 = String::from_utf8(charts_p5).unwrap();
        assert!(str_p5.contains("rankBefore:8|rankAfter:5|maxComboBefore:"));

        // Test combo not showing NEW when higher was achieved in a different submitted score
        let mut lower_combo_score = score.clone();
        lower_combo_score.max_combo = 300;
        let chart_bytes_existing = build_charts(&bmap, &lower_combo_score, &before, &after, Some(&score), Some(1), Some(1), Some(800), 5);
        let chart_str_existing = String::from_utf8(chart_bytes_existing).unwrap();
        // maxComboBefore and maxComboAfter both retain 800 -> no NEW combo
        assert!(chart_str_existing.contains("maxComboBefore:800|maxComboAfter:800"));

        // When beating previous max combo (e.g. 950 > 800)
        let mut higher_combo_score = score.clone();
        higher_combo_score.max_combo = 950;
        let chart_bytes_new = build_charts(&bmap, &higher_combo_score, &before, &after, Some(&score), Some(1), Some(1), Some(800), 5);
        let chart_str_new = String::from_utf8(chart_bytes_new).unwrap();
        assert!(chart_str_new.contains("maxComboBefore:800|maxComboAfter:950"));
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
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Failed play");

        let s = shared_state.read().await;
        let p = s.player.as_ref().unwrap();
        assert!(p.queue.is_empty(), "Failed plays must NOT send notification popups to the player");
    }
}
