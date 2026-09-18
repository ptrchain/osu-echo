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

#[allow(clippy::too_many_arguments)]
pub fn build_charts(
    bmap: &Beatmap,
    score: &Score,
    before: &ProfileStats,
    after: &ProfileStats,
    previous: Option<&Score>,
    rank_before: Option<i32>,
    rank_after: i32,
    playcount: i32,
) -> Vec<u8> {
    let meta = format!(
        "beatmapId:{}|beatmapSetId:{}|beatmapPlaycount:{}|beatmapPasscount:{}|approvedDate:0",
        bmap.beatmap_id, bmap.beatmapset_id, playcount, playcount
    );

    let prev_score_str = previous.map(|s| s.score.to_string()).unwrap_or_default();
    let prev_max_combo = previous.map(|s| s.max_combo.to_string()).unwrap_or_default();
    let prev_acc = previous.map(|s| format!("{:.2}", s.acc.unwrap_or(0.0))).unwrap_or_default();
    let prev_pp = previous.map(|s| format!("{:.0}", s.pp.unwrap_or(0.0))).unwrap_or_default();
    let prev_rank = rank_before.map(|r| r.to_string()).unwrap_or_default();

    let curr_score_str = score.score.to_string();
    let curr_max_combo = score.max_combo.to_string();
    let curr_acc = format!("{:.2}", score.acc.unwrap_or(0.0));
    let curr_pp = format!("{:.0}", score.pp.unwrap_or(0.0));
    let curr_rank = rank_after.to_string();

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
                let stats = ProfileStats {
                    rank: player.rank,
                    pp: player.pp as f64,
                    acc: player.acc,
                    ranked_score: player.ranked_score,
                    total_score: player.total_score,
                    max_combo: player.map_id,
                };
                let charts = build_charts(&bmap, existing_score.as_ref().unwrap_or(&sub.score), &stats, &stats, existing_score.as_ref(), Some(1), 1, playcount);
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

    if let Ok(parsed_map) = rosu_pp::Beatmap::from_bytes(content.as_bytes()) {
        let game_mode = match score.mode {
            0 => rosu_pp::model::mode::GameMode::Osu,
            1 => rosu_pp::model::mode::GameMode::Taiko,
            2 => rosu_pp::model::mode::GameMode::Catch,
            3 => rosu_pp::model::mode::GameMode::Mania,
            _ => rosu_pp::model::mode::GameMode::Osu,
        };

        let result = rosu_pp::Performance::new(&parsed_map)
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

    let before_stats = ProfileStats {
        rank: player.rank,
        pp: player.pp as f64,
        acc: player.acc,
        ranked_score: player.ranked_score,
        total_score: player.total_score,
        max_combo: 0,
    };

    let player_name = player.name.clone();
    let mode = s.mode;
    let ping_recent = s.config.ping_user_when_recent_score;
    let enable_recent = s.config.enable_recent_channel;

    let (previous, rank_before) = {
        let db_conn = s.db.lock().await;
        let prev_scores = db::get_scores_on_map(&db_conn, &player_name, &score.md5, score.mode).unwrap_or_default();
        let prev = prev_scores.into_iter().max_by_key(|s| s.score);
        let r_b = if prev.is_some() { Some(1) } else { None };
        (prev, r_b)
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
            max_combo: score.max_combo,
        };

        let rank_after = 1;
        let charts = build_charts(&bmap, &score, &before_stats, &after_stats, previous.as_ref(), rank_before, rank_after, playcount);

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

        let charts_bytes = build_charts(&bmap, &score, &before, &after, None, None, 1, 5);
        let charts_str = String::from_utf8(charts_bytes).unwrap();

        assert!(charts_str.contains("beatmapId:123|beatmapSetId:456|beatmapPlaycount:5|beatmapPasscount:5"));
        assert!(charts_str.contains("chartId:beatmap|chartUrl:https://osu.ppy.sh/b/123|chartName:Local Beatmap Ranking"));
        assert!(charts_str.contains("rankAfter:1"));
        assert!(charts_str.contains("onlineScoreId:-10"));
        assert!(charts_str.contains("chartId:overall"));
        assert!(charts_str.contains("rankBefore:100|rankAfter:95"));
        assert!(charts_str.contains("ppBefore:1000|ppAfter:1050"));
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
