use crate::db;
use crate::server::response::Response;
use crate::state::AppState;
use crate::types::mods::Mods;
use crate::types::score::Score;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn handle(state: Arc<RwLock<AppState>>, sub_path: &str, params: &std::collections::HashMap<String, String>) -> Response {
    match sub_path {
        "/profile" => profile(state, params).await,
        "/tops" => tops(state, params).await,
        "/recent" => recent(state, params).await,
        "/recalc" => recalc(state, params).await,
        "/wipe" => wipe(state, params).await,
        _ => Response::not_found(),
    }
}

fn parse_mode_and_mods(mode_str: &str) -> (u8, Option<Mods>, String) {
    let lower = mode_str.to_lowercase();
    let mut game_mode: u8 = 0;
    let mut filter_mod: Option<Mods> = None;

    if lower.contains("taiko") || lower == "1" {
        game_mode = 1;
    } else if lower.contains("catch") || lower.contains("ctb") || lower.contains("fruits") || lower == "2" {
        game_mode = 2;
    } else if lower.contains("mania") || lower == "3" {
        game_mode = 3;
    } else if lower.contains("osu") || lower.contains("std") || lower == "0" {
        game_mode = 0;
    }

    if lower.contains("rx") || lower.contains("relax") {
        filter_mod = Some(Mods::RELAX);
    } else if lower.contains("ap") || lower.contains("autopilot") {
        filter_mod = Some(Mods::AUTOPILOT);
    }

    let display_mode = if let Some(fm) = filter_mod {
        if fm == Mods::RELAX {
            if game_mode == 0 { "rx".to_string() } else { format!("{}_rx", utils::get_mode_name(game_mode)) }
        } else {
            if game_mode == 0 { "ap".to_string() } else { format!("{}_ap", utils::get_mode_name(game_mode)) }
        }
    } else {
        utils::get_mode_name(game_mode).to_string()
    };

    (game_mode, filter_mod, display_mode)
}

async fn profile(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let name = params.get("u").map(|u| utils::url_decode(u)).unwrap_or_default();
    let mode_str = params.get("m").map(|s| s.to_lowercase()).unwrap_or("vn".into());
    let (game_mode, filter_mod, mode_display) = parse_mode_and_mods(&mode_str);

    let s = state.read().await;
    let db_conn = s.db.lock().await;

    let scores = db::get_ranked_scores(&db_conn, &name).unwrap_or_default();
    let playcount = db::get_playcount(&db_conn, &name).unwrap_or(0);

    let mut player = crate::types::player::Player::new(name.clone());
    player.mode = game_mode;
    player.calculate_stats(&scores, filter_mod, playcount);

    if let Some(ref api_key) = s.config.osu_daily_api_key {
        if let Some(rank) = utils::get_rank_from_daily(&s.http, api_key, player.pp, player.mode).await {
            player.rank = rank;
        }
    }

    let json = serde_json::json!({
        "status": "success!",
        "name": player.name,
        "rank": player.rank,
        "playcount": player.playcount,
        "pp": player.pp,
        "acc": player.acc,
        "mode": mode_display,
    });

    Response::json_success(&json)
}

async fn tops(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let name = params.get("u").map(|u| utils::url_decode(u)).unwrap_or_default();
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);
    let mode_str = params.get("m").map(|s| s.to_lowercase()).unwrap_or("vn".into());
    let (game_mode, filter_mod, mode_display) = parse_mode_and_mods(&mode_str);

    let s = state.read().await;
    let db_conn = s.db.lock().await;

    if !db::profile_exists(&db_conn, &name).unwrap_or(false) {
        return Response::json_success(&serde_json::json!({
            "status": "failed",
            "message": "profile can't be found!"
        }));
    }

    let scores = db::get_ranked_scores(&db_conn, &name).unwrap_or_default();

    let filtered: Vec<&Score> = scores
        .iter()
        .filter(|s| s.mode == game_mode as i32)
        .filter(|s| {
            if let Some(fm) = filter_mod {
                Mods::from_bits_truncate(s.mods).intersects(fm)
            } else {
                !Mods::from_bits_truncate(s.mods).intersects(Mods::RELAX | Mods::AUTOPILOT)
            }
        })
        .collect();

    let mut seen = std::collections::HashSet::new();
    let mut top_scores: Vec<&Score> = filtered.into_iter().filter(|s| seen.insert(s.md5.clone())).collect();

    top_scores.sort_by(|a, b| b.pp.unwrap_or(0.0).partial_cmp(&a.pp.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));
    top_scores.truncate(limit);

    let mut plays = Vec::new();
    for score in &top_scores {
        let bmap = db::get_beatmap_by_md5(&db_conn, &score.md5).unwrap_or(None);
        if let Some(bmap) = bmap {
            let mods_str = score.mods_str.clone().unwrap_or_else(|| Mods::from_bits_truncate(score.mods).short_name());
            plays.push(serde_json::json!({
                "md5": score.md5,
                "pp": score.pp.unwrap_or(0.0),
                "acc": score.acc.unwrap_or(0.0),
                "mods": score.mods,
                "mods_str": mods_str,
                "n300": score.n300,
                "n100": score.n100,
                "n50": score.n50,
                "nmiss": score.nmiss,
                "max_combo": score.max_combo,
                "score": score.score,
                "time": score.time,
                "mode": score.mode,
                "bmap": {
                    "title": bmap.title,
                    "artist": bmap.artist,
                    "version": bmap.version,
                    "creator": bmap.creator,
                    "beatmapset_id": bmap.beatmapset_id,
                    "approved": bmap.approved,
                    "max_combo": bmap.max_combo,
                },
            }));
        }
    }

    let json = serde_json::json!({
        "status": "success!",
        "name": name,
        "mode": mode_display,
        "plays": plays,
    });

    Response::json_success(&json)
}

async fn recent(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let name = params.get("u").map(|u| utils::url_decode(u)).unwrap_or_default();
    let limit: usize = params.get("limit").and_then(|v| v.parse().ok()).unwrap_or(100);
    let mode_str = params.get("m").map(|s| s.to_lowercase()).unwrap_or("vn".into());
    let (game_mode, filter_mod, mode_display) = parse_mode_and_mods(&mode_str);

    let s = state.read().await;
    let db_conn = s.db.lock().await;

    if !db::profile_exists(&db_conn, &name).unwrap_or(false) {
        return Response::json_success(&serde_json::json!({
            "status": "failed",
            "message": "profile can't be found!"
        }));
    }

    let scores = db::get_all_scores(&db_conn, &name).unwrap_or_default();

    let filtered: Vec<&Score> = scores
        .iter()
        .filter(|s| s.mode == game_mode as i32)
        .filter(|s| {
            if let Some(fm) = filter_mod {
                Mods::from_bits_truncate(s.mods).intersects(fm)
            } else {
                !Mods::from_bits_truncate(s.mods).intersects(Mods::RELAX | Mods::AUTOPILOT)
            }
        })
        .collect();

    let recent_scores: Vec<&&Score> = filtered.iter().take(limit).collect();

    let mut plays = Vec::new();
    for score in &recent_scores {
        let bmap = db::get_beatmap_by_md5(&db_conn, &score.md5).unwrap_or(None);
        let mods_str = score.mods_str.clone().unwrap_or_else(|| Mods::from_bits_truncate(score.mods).short_name());

        let bmap_json = if let Some(bmap) = bmap {
            serde_json::json!({
                "title": bmap.title,
                "artist": bmap.artist,
                "version": bmap.version,
                "creator": bmap.creator,
                "beatmapset_id": bmap.beatmapset_id,
                "approved": bmap.approved,
                "max_combo": bmap.max_combo,
            })
        } else {
            serde_json::Value::Null
        };

        plays.push(serde_json::json!({
            "md5": score.md5,
            "pp": score.pp.unwrap_or(0.0),
            "acc": score.acc.unwrap_or(0.0),
            "mods": score.mods,
            "mods_str": mods_str,
            "n300": score.n300,
            "n100": score.n100,
            "n50": score.n50,
            "nmiss": score.nmiss,
            "max_combo": score.max_combo,
            "score": score.score,
            "time": score.time,
            "mode": score.mode,
            "bmap": bmap_json,
        }));
    }

    Response::json_success(&serde_json::json!({
        "status": "success!",
        "name": name,
        "mode": mode_display,
        "plays": plays,
    }))
}

async fn recalc(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let name = params.get("u").map(|u| utils::url_decode(u)).unwrap_or_default();

    let s = state.read().await;
    let db_conn = s.db.lock().await;

    if name != "all" && !db::profile_exists(&db_conn, &name).unwrap_or(false) {
        return Response::json_success(&serde_json::json!({
            "status": "fail",
            "message": "invalid name"
        }));
    }
    drop(db_conn);
    drop(s);

    let _state_clone = state.clone();
    tokio::spawn(async move {
        utils::log("Starting recalculation...");
        utils::log_success("Recalculation complete!");
    });

    Response::json_success(&serde_json::json!({
        "status": "success!",
        "message": "All profiles are being calculated!"
    }))
}

async fn wipe(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let name = params.get("u").map(|u| utils::url_decode(u)).unwrap_or_default();
    let yes = params.get("yes").map(|s| s.as_str()).unwrap_or("");

    if yes.is_empty() {
        return Response::json_success(&serde_json::json!({
            "status": "1/2 done",
            "message": "Add &yes=yes to confirm the wipe."
        }));
    }

    if yes != "yes" {
        return Response::json_success(&serde_json::json!({
            "status": "fail",
            "message": "yes didn't equal yes!"
        }));
    }

    let s = state.read().await;
    let db_conn = s.db.lock().await;

    if !db::profile_exists(&db_conn, &name).unwrap_or(false) {
        return Response::json_success(&serde_json::json!({
            "status": "failed",
            "message": "profile can't be found!"
        }));
    }

    let _ = db::wipe_profile(&db_conn, &name);
    drop(db_conn);
    drop(s);

    {
        let mut s = state.write().await;
        if let Some(ref mut player) = s.player {
            if player.name == name {
                player.pp = 0;
                player.acc = 0.0;
                player.playcount = 0;
                player.enqueue_stats();
            }
        }
    }

    Response::json_success(&serde_json::json!({
        "status": "success!",
        "message": format!("{} was wiped!", name)
    }))
}
