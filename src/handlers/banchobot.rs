use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db;
use crate::handlers::tillerino;
use crate::packets;
use crate::state::AppState;
use crate::types::beatmap::Beatmap;
use crate::types::mods::Mods;
use crate::types::player::Player;
use crate::utils;

pub const BOT_ID: i32 = 3;
pub const BOT_NAME: &str = "BanchoBot";

pub async fn reply(state: &Arc<RwLock<AppState>>, target: &str, msg: &str) {
    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        let pkt = packets::send_msg("BanchoBot", msg, target, BOT_ID);
        p.queue.extend_from_slice(&pkt);
    }
}

pub fn roll_dice(max: u32) -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().subsec_nanos();
    (nanos % max) + 1
}

pub async fn handle_command(state: &Arc<RwLock<AppState>>, player_name: &str, reply_target: &str, cmd: &str, args: &[&str]) {
    match cmd {
        "help" | "commands" => handle_help(state, reply_target).await,
        "recent" | "r" => handle_recent(state, player_name, reply_target).await,
        "tops" | "top" | "t" => handle_tops(state, player_name, reply_target).await,
        "stats" | "profile" => handle_stats(state, player_name, reply_target, args).await,
        "status" => handle_status(state, reply_target, args).await,
        "rank" => handle_set_status(state, reply_target, args, 1, "Ranked").await,
        "love" => handle_set_status(state, reply_target, args, 4, "Loved").await,
        "unrank" => handle_set_status(state, reply_target, args, 0, "Unranked").await,
        "friend" => handle_friend(state, player_name, reply_target, args).await,
        "recalc" | "recalculate" => handle_recalc(state, player_name, reply_target).await,
        "wipe" => handle_wipe(state, player_name, reply_target).await,
        "avatar" => handle_avatar(state, player_name, reply_target, args).await,
        "recentfeed" | "recentchannel" => handle_recent_channel_toggle(state, reply_target, args).await,
        "config" => handle_config(state, reply_target).await,
        "roll" => handle_roll(state, player_name, reply_target, args).await,
        "mode" => handle_mode(state, player_name, reply_target, args).await,
        "country" | "flag" => handle_country(state, player_name, reply_target, args).await,
        "mybest" | "pb" => handle_mybest(state, player_name, reply_target).await,
        "leaderboard" | "lb" => handle_leaderboard(state, player_name, reply_target).await,
        "restrictself" | "restrict" => handle_restrictself(state, player_name, reply_target, args).await,
        "unrestrictself" | "unrestrict" => handle_unrestrictself(state, player_name, reply_target).await,
        _ => {
            reply(state, reply_target, &format!("Unknown command: !{}. Type !help for commands.", cmd)).await;
        }
    }
}

pub async fn handle_help(state: &Arc<RwLock<AppState>>, target: &str) {
    let p = {
        let s = state.read().await;
        s.config.command_prefix.clone()
    };

    let msg = format!(
        "BanchoBot Commands:\n\
        {p}status / {p}rank / {p}love / {p}unrank [set/diff] [id] : Manage beatmap status (set or diff)\n\
        {p}recent / {p}r : Show your recent play\n\
        {p}tops / {p}t : Show top 5 plays\n\
        {p}stats / {p}profile : Show player stats\n\
        {p}mybest / {p}pb : Show your best score on the current map\n\
        {p}leaderboard / {p}lb : Show top scores on current map\n\
        {p}friend <add/remove/list/sync> : Manage friends\n\
        {p}mode <vn/rx/ap> : Switch game mode\n\
        {p}recentfeed [on/off] : Toggle #recent score channel feed\n\
        {p}country / {p}flag <code> : Set country flag (e.g. {p}country DE, {p}country US, {p}country AU)\n\
        {p}roll [max] : Roll a random number (default 100)\n\
        {p}recalc / {p}recalculate : Recalculate all profile stats & scores\n\
        {p}wipe : Wipe profile stats\n\
        {p}restrictself [reason] / {p}unrestrict : Mimic being banned on official osu! (toggle on/off)\n\
        {p}avatar <url or path> : Change avatar\n\
        {p}config : Server settings summary\n\n\
        Tillerino Commands (PM Tillerino or in chat):\n\
        /np : Beatmap info and PP breakdown\n\
        {p}r / {p}recommend : Recommend a beatmap\n\
        {p}with <mods> : PP with mods (e.g. {p}with HDHR)\n\
        {p}acc <val> : PP for accuracy (e.g. {p}acc 98.5)"
    );

    reply(state, target, &msg).await;
}

pub async fn handle_stats(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, _args: &[&str]) {
    let s = state.read().await;
    let Some(ref player) = s.player else {
        return;
    };

    let mode_name = match player.mode {
        0 => "osu!",
        1 => "Taiko",
        2 => "Catch",
        3 => "Mania",
        _ => "osu!",
    };

    let mod_suffix = match s.mode {
        Some(Mods::RELAX) => " (RX)",
        Some(Mods::AUTOPILOT) => " (AP)",
        _ => "",
    };

    let msg = format!(
        "Stats for {} ({}{}) | Rank: #{} | PP: {}pp | Acc: {:.2}% | Plays: {} | Ranked Score: {}",
        player_name, mode_name, mod_suffix, player.rank, player.pp, player.acc, player.playcount, player.ranked_score
    );

    drop(s);
    reply(state, target, &msg).await;
}

pub async fn handle_recent(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let (recent, bmap) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_all_scores(&db_conn, player_name).unwrap_or_default();
        let r = scores.first().cloned();
        let b = if let Some(ref sc) = r { db::get_beatmap_by_md5(&db_conn, &sc.md5).unwrap_or(None) } else { None };
        (r, b)
    };

    let Some(recent) = recent else {
        reply(state, target, "No recent scores found.").await;
        return;
    };

    let map_title = if let Some(ref b) = bmap { format!("{} - {} [{}]", b.artist, b.title, b.version) } else { format!("MD5: {}", recent.md5) };

    let mods_obj = Mods::from_bits_truncate(recent.mods);
    let mods_str = if mods_obj.is_empty() { String::new() } else { format!(" +{}", mods_obj.short_name()) };

    let pp_str = recent.pp.map(|p| format!("{:.0}pp", p)).unwrap_or_else(|| "0pp".to_string());
    let grade = utils::get_grade(
        recent.mode as u8,
        recent.n300,
        recent.n100,
        recent.n50,
        recent.ngeki,
        recent.nkatu,
        recent.nmiss,
        recent.mods,
        recent.acc.unwrap_or(0.0),
    );

    let msg = format!(
        "Recent Play: {}{} | Score: {} | Combo: {}x | Acc: {:.2}% | [{}/{}/{}] Miss: {} | {} ({})",
        map_title,
        mods_str,
        recent.score,
        recent.max_combo,
        recent.acc.unwrap_or(0.0),
        recent.n300,
        recent.n100,
        recent.n50,
        recent.nmiss,
        pp_str,
        grade
    );

    reply(state, target, &msg).await;
}

pub async fn handle_tops(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let (scores, maps) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scs = db::get_ranked_scores(&db_conn, player_name).unwrap_or_default();
        let mut scores = Vec::new();
        let mut bmaps = Vec::new();
        let mut seen_md5 = std::collections::HashSet::new();
        for sc in scs {
            if seen_md5.insert(sc.md5.clone()) {
                scores.push(sc);
            }
            if scores.len() >= 5 {
                break;
            }
        }
        for sc in scores.iter() {
            let b = db::get_beatmap_by_md5(&db_conn, &sc.md5).unwrap_or(None);
            bmaps.push(b);
        }
        (scores, bmaps)
    };

    if scores.is_empty() {
        reply(state, target, "No top plays submitted yet!").await;
        return;
    }

    let mut lines = Vec::new();
    lines.push(format!("Top plays for {}:", player_name));

    for (i, sc) in scores.iter().take(5).enumerate() {
        let title = maps.get(i).and_then(|b| b.as_ref()).map(|b| format!("{} - {} [{}]", b.artist, b.title, b.version)).unwrap_or_else(|| sc.md5.clone());
        let mods_obj = Mods::from_bits_truncate(sc.mods);
        let mods_str = if mods_obj.is_empty() { String::new() } else { format!(" +{}", mods_obj.short_name()) };
        lines.push(format!("#{}: {}{} | {:.2}% | {:.0}pp", i + 1, title, mods_str, sc.acc.unwrap_or(0.0), sc.pp.unwrap_or(0.0)));
    }

    reply(state, target, &lines.join("\n")).await;
}

pub async fn handle_mybest(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let (map_md5, map_title, mode) = {
        let s = state.read().await;
        let mut md5 = String::new();
        let mut title = String::new();
        let mode = s.player.as_ref().map(|p| p.mode).unwrap_or(0);

        if let Some(ref p) = s.player {
            if !p.map_md5.is_empty() {
                md5 = p.map_md5.clone();
                title = p.info_text.clone();
                if let Some(ref b) = s.last_np_map {
                    if b.file_md5 == md5 {
                        title = format!("{} - {} [{}]", b.artist, b.title, b.version);
                    }
                }
            }
        }
        if md5.is_empty() {
            if let Some(ref b) = s.last_np_map {
                md5 = b.file_md5.clone();
                title = format!("{} - {} [{}]", b.artist, b.title, b.version);
            }
        }
        (md5, title, mode)
    };

    if map_md5.is_empty() {
        reply(state, target, "No beatmap selected! Use /np or select a map first.").await;
        return;
    }

    let scores = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        db::get_scores_on_map(&db_conn, player_name, &map_md5, mode as i32).unwrap_or_default()
    };

    let Some(best) = scores.first() else {
        reply(state, target, "You haven't set any scores on this beatmap yet!").await;
        return;
    };

    let mods_obj = Mods::from_bits_truncate(best.mods);
    let mods_str = if mods_obj.is_empty() { String::new() } else { format!(" +{}", mods_obj.short_name()) };
    let title_display = if map_title.is_empty() { map_md5 } else { map_title };

    let msg = format!(
        "Personal Best on {}{}: Score: {} | Combo: {}x | Acc: {:.2}% | {:.0}pp",
        title_display,
        mods_str,
        best.score,
        best.max_combo,
        best.acc.unwrap_or(0.0),
        best.pp.unwrap_or(0.0)
    );
    reply(state, target, &msg).await;
}

pub async fn handle_leaderboard(state: &Arc<RwLock<AppState>>, _player_name: &str, target: &str) {
    let (map_md5, map_title, mode, player_name) = {
        let s = state.read().await;
        let mut md5 = String::new();
        let mut title = String::new();
        let mode = s.player.as_ref().map(|p| p.mode).unwrap_or(0);
        let name = s.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();

        if let Some(ref p) = s.player {
            if !p.map_md5.is_empty() {
                md5 = p.map_md5.clone();
                title = p.info_text.clone();
                if let Some(ref b) = s.last_np_map {
                    if b.file_md5 == md5 {
                        title = format!("{} - {} [{}]", b.artist, b.title, b.version);
                    }
                }
            }
        }
        if md5.is_empty() {
            if let Some(ref b) = s.last_np_map {
                md5 = b.file_md5.clone();
                title = format!("{} - {} [{}]", b.artist, b.title, b.version);
            }
        }
        (md5, title, mode, name)
    };

    if map_md5.is_empty() {
        reply(state, target, "No beatmap selected! Use /np or select a map first.").await;
        return;
    }

    let scores = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        db::get_scores_on_map(&db_conn, &player_name, &map_md5, mode as i32).unwrap_or_default()
    };

    if scores.is_empty() {
        reply(state, target, "No scores found on this map.").await;
        return;
    }

    let title_display = if map_title.is_empty() { map_md5 } else { map_title };
    let mut lines = Vec::new();
    lines.push(format!("Top scores on {}:", title_display));

    for (i, sc) in scores.iter().take(5).enumerate() {
        let mods_obj = Mods::from_bits_truncate(sc.mods);
        let mods_str = if mods_obj.is_empty() { String::new() } else { format!(" +{}", mods_obj.short_name()) };
        lines.push(format!("#{}: {} | {}{} | {:.2}% | {:.0}pp", i + 1, sc.name, sc.score, mods_str, sc.acc.unwrap_or(0.0), sc.pp.unwrap_or(0.0)));
    }

    reply(state, target, &lines.join("\n")).await;
}

// Resolves the target beatmap ID for bot commands (e.g. !rank, !status).
// Prioritizes the active player map MD5 over stale state to prevent ranking the wrong beatmap.
// This logic fixes bug #B0001 (stale map selection / incorrect PP on multi-diff mapsets).
// Thanks to kaan for reporting it!
pub async fn resolve_target_map_id(state: &Arc<RwLock<AppState>>, args: &[&str]) -> Option<i64> {
    if let Some(id) = args.iter().find_map(|a| a.parse::<i64>().ok()) {
        return Some(id);
    }

    let (player_md5, player_mid, last_np) = {
        let s = state.read().await;
        let md5 = s.player.as_ref().map(|p| p.map_md5.clone()).unwrap_or_default();
        let mid = s.player.as_ref().map(|p| p.map_id as i64).unwrap_or(0);
        let np = s.last_np_map.clone();
        (md5, mid, np)
    };

    if !player_md5.is_empty() {
        if let Some(ref np) = last_np {
            if np.file_md5.eq_ignore_ascii_case(&player_md5) && np.beatmap_id > 0 {
                return Some(np.beatmap_id);
            }
        }

        let s = state.read().await;
        let db_conn = s.db.lock().await;
        if let Ok(Some(b)) = db::get_beatmap_by_md5(&db_conn, &player_md5) {
            if b.beatmap_id > 0 {
                return Some(b.beatmap_id);
            }
        }

        if let Some(songs_dir) = utils::resolve_songs_folder(&s.config) {
            if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(&songs_dir, None, None, Some(&player_md5), None) {
                let _ = db::insert_beatmap(&db_conn, &local_bmap);
                let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
                if local_bmap.beatmap_id > 0 {
                    return Some(local_bmap.beatmap_id);
                }
            }
        }

        let api_key = s.config.osu_api_key.clone();
        let http = s.http.clone();
        drop(db_conn);
        drop(s);
        if let Some(ref key) = api_key {
            if let Some(fetched) = utils::fetch_beatmap_from_api(&http, key, &[("h", player_md5.clone())]).await {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &fetched);
                if fetched.beatmap_id > 0 {
                    return Some(fetched.beatmap_id);
                }
            }
        }
    }

    if player_md5.is_empty() && player_mid > 0 {
        return Some(player_mid);
    }

    // Fallback to last_np_map only when player has no active map MD5
    if player_md5.is_empty() {
        if let Some(np) = last_np {
            if np.beatmap_id > 0 {
                return Some(np.beatmap_id);
            }
        }
    }

    None
}

pub async fn handle_set_status(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str], status: i32, status_name: &str) {
    let is_set = args.iter().any(|a| a.eq_ignore_ascii_case("set"));
    let number_arg = args.iter().find_map(|a| a.parse::<i64>().ok());

    if is_set {
        let (player_md5, player_mid, _player_info) = {
            let s = state.read().await;
            (
                s.player.as_ref().map(|p| p.map_md5.clone()).unwrap_or_default(),
                s.player.as_ref().map(|p| p.map_id as i64).unwrap_or(0),
                s.player.as_ref().map(|p| p.info_text.clone()).unwrap_or_default(),
            )
        };

        let mut target_set_id = 0i64;
        if let Some(num) = number_arg {
            target_set_id = num;
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            if let Ok(Some(b)) = db::get_beatmap_by_id(&db_conn, num) {
                if b.beatmapset_id > 0 {
                    target_set_id = b.beatmapset_id;
                }
            }
        } else {
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            if !player_md5.is_empty() {
                if let Ok(Some(b)) = db::get_beatmap_by_md5(&db_conn, &player_md5) {
                    if b.beatmapset_id > 0 {
                        target_set_id = b.beatmapset_id;
                    }
                }
            }
            if target_set_id == 0 {
                if let Some(ref np) = s.last_np_map {
                    if np.beatmapset_id > 0 {
                        target_set_id = np.beatmapset_id;
                    }
                }
            }
            if target_set_id == 0 && player_mid > 0 {
                if let Ok(Some(b)) = db::get_beatmap_by_id(&db_conn, player_mid) {
                    if b.beatmapset_id > 0 {
                        target_set_id = b.beatmapset_id;
                    }
                }
            }
        }

        let (songs_dir, api_key, http) = {
            let s = state.read().await;
            (utils::resolve_songs_folder(&s.config), s.config.osu_api_key.clone(), s.http.clone())
        };

        if target_set_id > 0 {
            if let Some(ref sdir) = songs_dir {
                if let Some(folder_path) = utils::find_local_mapset_folder(sdir, Some(target_set_id), None) {
                    let scanned = utils::scan_dir_for_all_osu_files(&folder_path, Some(target_set_id));
                    let s = state.read().await;
                    let db_conn = s.db.lock().await;
                    for (mut bmap, content) in scanned {
                        bmap.approved = status;
                        let _ = db::insert_beatmap(&db_conn, &bmap);
                        let _ = db::update_beatmap_file_content(&db_conn, &bmap.file_md5, &content);
                        let _ = db::set_beatmap_status_by_md5(&db_conn, &bmap.file_md5, status);
                        if bmap.beatmap_id > 0 {
                            let _ = db::set_beatmap_status(&db_conn, bmap.beatmap_id, status);
                        }
                    }
                }
            }

            if let Some(ref key) = api_key {
                if let Some(api_bmaps) = utils::fetch_all_beatmaps_from_api(&http, key, &[("s", target_set_id.to_string())]).await {
                    let s = state.read().await;
                    let db_conn = s.db.lock().await;
                    for mut bmap in api_bmaps {
                        bmap.approved = status;
                        let _ = db::insert_beatmap(&db_conn, &bmap);
                        let _ = db::set_beatmap_status(&db_conn, bmap.beatmap_id, status);
                        if !bmap.file_md5.is_empty() {
                            let _ = db::set_beatmap_status_by_md5(&db_conn, &bmap.file_md5, status);
                        }
                    }
                }
            }

            let set_bmaps = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                let _ = db::set_beatmapset_status(&db_conn, target_set_id, status);
                db::get_beatmaps_by_set_id(&db_conn, target_set_id).unwrap_or_default()
            };

            if !set_bmaps.is_empty() {
                let artist = set_bmaps[0].artist.clone();
                let title = set_bmaps[0].title.clone();
                let count = set_bmaps.len();

                let mut s = state.write().await;
                if let Some(ref mut np) = s.last_np_map {
                    if np.beatmapset_id == target_set_id {
                        np.approved = status;
                    }
                }
                drop(s);

                reply(state, target, &format!("Beatmapset {} - {} (Set ID: {}) ({} difficulties) is now {}!", artist, title, target_set_id, count, status_name)).await;
                return;
            }
        }

        if !player_md5.is_empty() {
            if let Some(ref sdir) = songs_dir {
                if let Some(folder_path) = utils::find_local_mapset_folder(sdir, None, Some(&player_md5)) {
                    let scanned = utils::scan_dir_for_all_osu_files(&folder_path, None);
                    let count = scanned.len();
                    let (mut artist, mut title) = (String::new(), String::new());
                    {
                        let s = state.read().await;
                        let db_conn = s.db.lock().await;
                        for (mut bmap, content) in scanned {
                            if artist.is_empty() {
                                artist = bmap.artist.clone();
                                title = bmap.title.clone();
                            }
                            bmap.approved = status;
                            let _ = db::insert_beatmap(&db_conn, &bmap);
                            let _ = db::update_beatmap_file_content(&db_conn, &bmap.file_md5, &content);
                            let _ = db::set_beatmap_status_by_md5(&db_conn, &bmap.file_md5, status);
                            if bmap.beatmap_id > 0 {
                                let _ = db::set_beatmap_status(&db_conn, bmap.beatmap_id, status);
                            }
                        }
                    }

                    if count > 0 {
                        let mut s = state.write().await;
                        if let Some(ref mut np) = s.last_np_map {
                            np.approved = status;
                        }
                        drop(s);

                        let display_artist = if artist.is_empty() { "Unknown Artist" } else { &artist };
                        let display_title = if title.is_empty() { "Custom Beatmap" } else { &title };
                        reply(state, target, &format!("Beatmapset {} - {} ({} difficulties) is now {}!", display_artist, display_title, count, status_name)).await;
                        return;
                    }
                }
            }
        }

        if target_set_id > 0 {
            reply(state, target, &format!("Beatmapset ID: {} not found in local database or Songs folder.", target_set_id)).await;
        } else {
            reply(state, target, &format!("Usage: !{} set [beatmapset_id], or select a map first.", status_name.to_lowercase())).await;
        }
        return;
    }

    let explicit_id = number_arg;

    let (player_md5, _player_mid, player_info) = {
        let s = state.read().await;
        (
            s.player.as_ref().map(|p| p.map_md5.clone()).unwrap_or_default(),
            s.player.as_ref().map(|p| p.map_id as i64).unwrap_or(0),
            s.player.as_ref().map(|p| p.info_text.clone()).unwrap_or_default(),
        )
    };

    // If no explicit ID argument is provided and player has an active map MD5:
    // rank the active map directly by MD5 to avoid turning practice diffs into top diffs!
    if explicit_id.is_none() && !player_md5.is_empty() {
        let (updated, bmap) = {
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            let mut b = db::get_beatmap_by_md5(&db_conn, &player_md5).ok().flatten();
            if b.is_none() {
                drop(db_conn);
                let songs_dir = utils::resolve_songs_folder(&s.config);
                let api_key = s.config.osu_api_key.clone();
                let http = s.http.clone();
                if let Some(ref sdir) = songs_dir {
                    if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(sdir, None, None, Some(&player_md5), None) {
                        let md5 = local_bmap.file_md5.clone();
                        let db_conn = s.db.lock().await;
                        let _ = db::insert_beatmap(&db_conn, &local_bmap);
                        let _ = db::update_beatmap_file_content(&db_conn, &md5, &content);
                        b = Some(local_bmap);
                    }
                }
                if b.is_none() {
                    if let Some(ref key) = api_key {
                        if let Some(fetched) = utils::fetch_beatmap_from_api(&http, key, &[("h", player_md5.clone())]).await {
                            let db_conn = s.db.lock().await;
                            let _ = db::insert_beatmap(&db_conn, &fetched);
                            b = Some(fetched);
                        }
                    }
                }
                if b.is_none() && !player_info.is_empty() {
                    let (artist, title, creator, version) = utils::parse_osu_filename(&player_info);
                    let mut fb = Beatmap::blank();
                    fb.file_md5 = player_md5.clone();
                    fb.artist = if artist.is_empty() { "Unknown Artist".to_string() } else { artist };
                    fb.title = if title.is_empty() { "Custom Beatmap".to_string() } else { title };
                    fb.creator = creator;
                    fb.version = if version.is_empty() { "Normal".to_string() } else { version };
                    let db_conn = s.db.lock().await;
                    let _ = db::insert_beatmap(&db_conn, &fb);
                    b = Some(fb);
                }
            } else {
                drop(db_conn);
            }

            if let Some(mut target_bmap) = b {
                target_bmap.approved = status;
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &target_bmap);
                let _ = db::set_beatmap_status_by_md5(&db_conn, &player_md5, status);
                if target_bmap.beatmap_id > 0 {
                    let is_canonical = db::get_beatmap_by_id(&db_conn, target_bmap.beatmap_id)
                        .ok()
                        .flatten()
                        .map_or(false, |c| c.file_md5.eq_ignore_ascii_case(&player_md5));
                    if is_canonical {
                        let _ = db::set_beatmap_status(&db_conn, target_bmap.beatmap_id, status);
                    }
                }
                (1, Some(target_bmap))
            } else {
                (0, None)
            }
        };

        if updated > 0 {
            if let Some(b) = bmap {
                let mut s = state.write().await;
                s.last_np_map = Some(b.clone());
                if let Some(ref mut p) = s.player {
                    p.map_id = b.beatmap_id as i32;
                    p.map_md5 = b.file_md5.clone();
                }
                drop(s);
                let id_str = if b.beatmap_id > 0 { format!(" (ID: {})", b.beatmap_id) } else { String::new() };
                reply(state, target, &format!("Beatmap {} - {} [{}]{} is now {}!", b.artist, b.title, b.version, id_str, status_name)).await;
            }
            return;
        }
    }

    let map_id = resolve_target_map_id(state, args).await;

    let Some(map_id) = map_id else {
        reply(state, target, &format!("Usage: !{} [set/diff] [beatmap_id], or select a map first.", status_name.to_lowercase())).await;
        return;
    };

    let (player_md5, player_info) = {
        let s = state.read().await;
        (
            s.player.as_ref().map(|p| p.map_md5.clone()).unwrap_or_default(),
            s.player.as_ref().map(|p| p.info_text.clone()).unwrap_or_default(),
        )
    };

    let (updated, bmap) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let mut u = db::set_beatmap_status(&db_conn, map_id, status).unwrap_or(0);
        let mut b = if u > 0 { db::get_beatmap_by_id(&db_conn, map_id).ok().flatten() } else { None };
        if u == 0 && !player_md5.is_empty() {
            if let Ok(Some(mut existing)) = db::get_beatmap_by_md5(&db_conn, &player_md5) {
                existing.approved = status;
                if existing.beatmap_id == 0 && map_id > 0 {
                    existing.beatmap_id = map_id;
                }
                let _ = db::insert_beatmap(&db_conn, &existing);
                let _ = db::set_beatmap_status_by_md5(&db_conn, &player_md5, status);
                u = 1;
                b = Some(existing);
            }
        }
        if u == 0 {
            drop(db_conn);
            let api_key = s.config.osu_api_key.clone();
            let http = s.http.clone();
            let songs_dir = utils::resolve_songs_folder(&s.config);
            let mut fetched_bmap = None;
            if let Some(ref key) = api_key {
                fetched_bmap = utils::fetch_beatmap_from_api(&http, key, &[("b", map_id.to_string())]).await;
                if fetched_bmap.is_none() && !player_md5.is_empty() {
                    fetched_bmap = utils::fetch_beatmap_from_api(&http, key, &[("h", player_md5.clone())]).await;
                }
            }
            if fetched_bmap.is_none() {
                if let Some(ref sdir) = songs_dir {
                    if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(sdir, None, Some(map_id), None, None) {
                        let md5 = local_bmap.file_md5.clone();
                        fetched_bmap = Some(local_bmap);
                        let db_conn = s.db.lock().await;
                        let _ = db::update_beatmap_file_content(&db_conn, &md5, &content);
                    } else if !player_md5.is_empty() {
                        if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(sdir, None, None, Some(&player_md5), None) {
                            let md5 = local_bmap.file_md5.clone();
                            fetched_bmap = Some(local_bmap);
                            let db_conn = s.db.lock().await;
                            let _ = db::update_beatmap_file_content(&db_conn, &md5, &content);
                        }
                    }
                }
            }
            if fetched_bmap.is_none() && (!player_md5.is_empty() || !player_info.is_empty()) {
                let (artist, title, creator, version) = if !player_info.is_empty() {
                    utils::parse_osu_filename(&player_info)
                } else {
                    (String::new(), format!("Beatmap {}", map_id), String::new(), String::new())
                };
                let mut fb = Beatmap::blank();
                fb.beatmap_id = map_id;
                fb.file_md5 = player_md5.clone();
                fb.artist = if artist.is_empty() { "Unknown Artist".to_string() } else { artist };
                fb.title = if title.is_empty() { format!("Beatmap {}", map_id) } else { title };
                fb.creator = creator;
                fb.version = if version.is_empty() { "Normal".to_string() } else { version };
                fb.approved = status;
                fetched_bmap = Some(fb);
            }
            if let Some(mut fb) = fetched_bmap {
                fb.approved = status;
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &fb);
                let _ = db::set_beatmap_status(&db_conn, map_id, status);
                if !fb.file_md5.is_empty() {
                    let _ = db::set_beatmap_status_by_md5(&db_conn, &fb.file_md5, status);
                }
                u = 1;
                b = Some(fb);
            }
        }
        (u, b)
    };

    if updated > 0 {
        if let Some(b) = bmap {
            let mut s = state.write().await;
            s.last_np_map = Some(b.clone());
            if let Some(ref mut p) = s.player {
                if p.map_md5.is_empty() || p.map_md5 == b.file_md5 || p.map_id == b.beatmap_id as i32 {
                    p.map_id = b.beatmap_id as i32;
                    p.map_md5 = b.file_md5.clone();
                }
            }
            drop(s);
            reply(state, target, &format!("Beatmap {} - {} [{}] (ID: {}) is now {}!", b.artist, b.title, b.version, map_id, status_name)).await;
        } else {
            reply(state, target, &format!("Beatmap ID: {} status updated to {}!", map_id, status_name)).await;
        }
    } else {
        reply(state, target, &format!("Beatmap ID: {} not found in local database. Select it in song select first.", map_id)).await;
    }
}

pub async fn handle_status(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    let is_set = args.iter().any(|a| a.eq_ignore_ascii_case("set"));
    let number_arg = args.iter().find_map(|a| a.parse::<i64>().ok());

    let (player_md5, player_mid, player_info) = {
        let s = state.read().await;
        (
            s.player.as_ref().map(|p| p.map_md5.clone()).unwrap_or_default(),
            s.player.as_ref().map(|p| p.map_id as i64).unwrap_or(0),
            s.player.as_ref().map(|p| p.info_text.clone()).unwrap_or_default(),
        )
    };

    if is_set {
        let mut target_set_id = 0i64;
        if let Some(num) = number_arg {
            target_set_id = num;
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            if let Ok(Some(b)) = db::get_beatmap_by_id(&db_conn, num) {
                if b.beatmapset_id > 0 {
                    target_set_id = b.beatmapset_id;
                }
            }
        } else {
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            if !player_md5.is_empty() {
                if let Ok(Some(b)) = db::get_beatmap_by_md5(&db_conn, &player_md5) {
                    if b.beatmapset_id > 0 {
                        target_set_id = b.beatmapset_id;
                    }
                }
            }
            if target_set_id == 0 {
                if let Some(ref np) = s.last_np_map {
                    if np.beatmapset_id > 0 {
                        target_set_id = np.beatmapset_id;
                    }
                }
            }
            if target_set_id == 0 && player_mid > 0 {
                if let Ok(Some(b)) = db::get_beatmap_by_id(&db_conn, player_mid) {
                    if b.beatmapset_id > 0 {
                        target_set_id = b.beatmapset_id;
                    }
                }
            }
        }

        let (songs_dir, api_key, http) = {
            let s = state.read().await;
            (utils::resolve_songs_folder(&s.config), s.config.osu_api_key.clone(), s.http.clone())
        };

        if target_set_id > 0 {
            let mut set_bmaps = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                db::get_beatmaps_by_set_id(&db_conn, target_set_id).unwrap_or_default()
            };

            if set_bmaps.is_empty() {
                if let Some(ref sdir) = songs_dir {
                    if let Some(folder_path) = utils::find_local_mapset_folder(sdir, Some(target_set_id), None) {
                        let scanned = utils::scan_dir_for_all_osu_files(&folder_path, Some(target_set_id));
                        let s = state.read().await;
                        let db_conn = s.db.lock().await;
                        for (bmap, content) in scanned {
                            let _ = db::insert_beatmap(&db_conn, &bmap);
                            let _ = db::update_beatmap_file_content(&db_conn, &bmap.file_md5, &content);
                        }
                    }
                }
                if let Some(ref key) = api_key {
                    if let Some(api_bmaps) = utils::fetch_all_beatmaps_from_api(&http, key, &[("s", target_set_id.to_string())]).await {
                        let s = state.read().await;
                        let db_conn = s.db.lock().await;
                        for bmap in api_bmaps {
                            let _ = db::insert_beatmap(&db_conn, &bmap);
                        }
                    }
                }
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                set_bmaps = db::get_beatmaps_by_set_id(&db_conn, target_set_id).unwrap_or_default();
            }

            if !set_bmaps.is_empty() {
                let artist = set_bmaps[0].artist.clone();
                let title = set_bmaps[0].title.clone();
                let total = set_bmaps.len();
                let ranked = set_bmaps.iter().filter(|b| b.approved == 1 || b.approved == 2).count();
                let loved = set_bmaps.iter().filter(|b| b.approved == 4).count();
                let qualified = set_bmaps.iter().filter(|b| b.approved == 3).count();
                let unranked = set_bmaps.iter().filter(|b| b.approved <= 0).count();

                let status_summary = if ranked == total {
                    "Ranked".to_string()
                } else if loved == total {
                    "Loved".to_string()
                } else if unranked == total {
                    "Unranked / Pending".to_string()
                } else {
                    let mut parts = Vec::new();
                    if ranked > 0 { parts.push(format!("{} Ranked", ranked)); }
                    if loved > 0 { parts.push(format!("{} Loved", loved)); }
                    if qualified > 0 { parts.push(format!("{} Qualified", qualified)); }
                    if unranked > 0 { parts.push(format!("{} Unranked", unranked)); }
                    parts.join(", ")
                };

                reply(state, target, &format!("Beatmapset {} - {} (Set ID: {}) ({} difficulties) is {}", artist, title, target_set_id, total, status_summary)).await;
                return;
            }
        }

        if !player_md5.is_empty() {
            if let Some(ref sdir) = songs_dir {
                if let Some(folder_path) = utils::find_local_mapset_folder(sdir, None, Some(&player_md5)) {
                    let scanned = utils::scan_dir_for_all_osu_files(&folder_path, None);
                    if !scanned.is_empty() {
                        let total = scanned.len();
                        let artist = scanned[0].0.artist.clone();
                        let title = scanned[0].0.title.clone();
                        let mut ranked = 0;
                        let mut loved = 0;
                        let mut unranked = 0;
                        {
                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            for (b, _) in &scanned {
                                if let Ok(Some(existing)) = db::get_beatmap_by_md5(&db_conn, &b.file_md5) {
                                    match existing.approved {
                                        1 | 2 => ranked += 1,
                                        4 => loved += 1,
                                        _ => unranked += 1,
                                    }
                                } else {
                                    unranked += 1;
                                }
                            }
                        }
                        let status_summary = if ranked == total {
                            "Ranked".to_string()
                        } else if loved == total {
                            "Loved".to_string()
                        } else if unranked == total {
                            "Unranked / Pending".to_string()
                        } else {
                            format!("{} Ranked, {} Unranked", ranked, unranked)
                        };
                        reply(state, target, &format!("Beatmapset {} - {} ({} difficulties) is {}", artist, title, total, status_summary)).await;
                        return;
                    }
                }
            }
        }

        if target_set_id > 0 {
            reply(state, target, &format!("Beatmapset ID: {} not found in database.", target_set_id)).await;
        } else {
            reply(state, target, "Usage: !status [set/diff] [id], or select a map first.").await;
        }
        return;
    }

    let explicit_id = number_arg;

    if explicit_id.is_none() && !player_md5.is_empty() {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let b = db::get_beatmap_by_md5(&db_conn, &player_md5).ok().flatten();
        drop(db_conn);
        drop(s);
        if let Some(b) = b {
            let status_str = match b.approved {
                1 => "Ranked",
                2 => "Approved",
                3 => "Qualified",
                4 => "Loved",
                _ => "Unranked / Pending",
            };
            let id_str = if b.beatmap_id > 0 { format!(" (ID: {})", b.beatmap_id) } else { String::new() };
            reply(state, target, &format!("{} - {} [{}]{} is {}", b.artist, b.title, b.version, id_str, status_str)).await;
            return;
        }
    }

    let map_id = resolve_target_map_id(state, args).await;

    let Some(map_id) = map_id else {
        reply(state, target, "Usage: !status [set/diff] [beatmap_id], or select a map first.").await;
        return;
    };

    let beatmap = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let mut b = db::get_beatmap_by_id(&db_conn, map_id).ok().flatten();
        if b.is_none() && !player_md5.is_empty() {
            b = db::get_beatmap_by_md5(&db_conn, &player_md5).ok().flatten();
        }
        if b.is_none() {
            drop(db_conn);
            let api_key = s.config.osu_api_key.clone();
            let http = s.http.clone();
            let songs_dir = utils::resolve_songs_folder(&s.config);
            if let Some(ref key) = api_key {
                b = utils::fetch_beatmap_from_api(&http, key, &[("b", map_id.to_string())]).await;
                if b.is_none() && !player_md5.is_empty() {
                    b = utils::fetch_beatmap_from_api(&http, key, &[("h", player_md5.clone())]).await;
                }
            }
            if b.is_none() {
                if let Some(ref sdir) = songs_dir {
                    if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(sdir, None, Some(map_id), None, None) {
                        let db_conn = s.db.lock().await;
                        let _ = db::insert_beatmap(&db_conn, &local_bmap);
                        let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
                        b = Some(local_bmap);
                    } else if !player_md5.is_empty() {
                        if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(sdir, None, None, Some(&player_md5), None) {
                            let db_conn = s.db.lock().await;
                            let _ = db::insert_beatmap(&db_conn, &local_bmap);
                            let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
                            b = Some(local_bmap);
                        }
                    }
                }
            }
            if b.is_none() && (!player_md5.is_empty() || !player_info.is_empty()) {
                let (artist, title, creator, version) = if !player_info.is_empty() {
                    utils::parse_osu_filename(&player_info)
                } else {
                    (String::new(), format!("Beatmap {}", map_id), String::new(), String::new())
                };
                let mut fb = Beatmap::blank();
                fb.beatmap_id = map_id;
                fb.file_md5 = player_md5.clone();
                fb.artist = if artist.is_empty() { "Unknown Artist".to_string() } else { artist };
                fb.title = if title.is_empty() { format!("Beatmap {}", map_id) } else { title };
                fb.creator = creator;
                fb.version = if version.is_empty() { "Normal".to_string() } else { version };
                fb.approved = 0;
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &fb);
                b = Some(fb);
            } else if let Some(ref fb) = b {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, fb);
            }
        }
        b
    };

    if let Some(b) = beatmap {
        let status_str = match b.approved {
            1 => "Ranked",
            2 => "Approved",
            3 => "Qualified",
            4 => "Loved",
            _ => "Unranked / Pending",
        };
        reply(state, target, &format!("{} - {} [{}] (ID: {}) is {}", b.artist, b.title, b.version, map_id, status_str)).await;
    } else {
        reply(state, target, &format!("Beatmap ID: {} not found in database.", map_id)).await;
    }
}

pub async fn handle_friend(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    if args.is_empty() {
        reply(state, target, "Usage: !friend <add/remove/list/sync> [user]").await;
        return;
    }

    let subcmd = args[0].to_lowercase();
    match subcmd.as_str() {
        "add" => {
            if args.len() < 2 {
                reply(state, target, "Usage: !friend add <username or user_id>").await;
                return;
            }
            let user_query = args[1..].join(" ");
            let (api_key, http) = {
                let s = state.read().await;
                (s.config.osu_api_key.clone(), s.http.clone())
            };

            let is_tillerino_query = user_query.eq_ignore_ascii_case("tillerino") || user_query == "4" || user_query == "2070907";
            let is_banchobot_query = user_query.eq_ignore_ascii_case("banchobot") || user_query == "3";

            let mut fetched_friend: Option<db::FriendRecord> = None;
            if !is_tillerino_query && !is_banchobot_query {
                if let Some(ref key) = api_key {
                    fetched_friend = utils::fetch_user_stats_from_api(&http, key, &user_query).await;
                }
            }

            let friend_rec = if is_tillerino_query {
                db::FriendRecord {
                    friend_id: tillerino::BOT_ID,
                    friend_name: tillerino::BOT_NAME.to_string(),
                    rank: 1,
                    pp: 0,
                    acc: 0.0,
                    country: 0,
                    ranked_score: 0,
                    total_score: 0,
                    playcount: 0,
                }
            } else if is_banchobot_query {
                db::FriendRecord {
                    friend_id: BOT_ID,
                    friend_name: BOT_NAME.to_string(),
                    rank: 1,
                    pp: 0,
                    acc: 0.0,
                    country: 0,
                    ranked_score: 0,
                    total_score: 0,
                    playcount: 0,
                }
            } else if let Some(rec) = fetched_friend {
                rec
            } else if let Ok(uid) = user_query.parse::<i32>() {
                db::FriendRecord {
                    friend_id: uid,
                    friend_name: format!("Friend {}", uid),
                    rank: 1,
                    pp: 0,
                    acc: 0.0,
                    country: 0,
                    ranked_score: 0,
                    total_score: 0,
                    playcount: 0,
                }
            } else {
                reply(state, target, &format!("Could not find user '{}'. Try providing their numerical user ID.", user_query)).await;
                return;
            };

            let player_userid = {
                let s = state.read().await;
                s.player.as_ref().map(|p| p.userid).unwrap_or(2)
            };

            if friend_rec.friend_id <= 2 || friend_rec.friend_id == player_userid {
                reply(state, target, "You cannot add yourself to your friends list!").await;
                return;
            }

            let mut friend_ids = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                let _ = db::save_friend_stats(&db_conn, player_name, &friend_rec);
                db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
            };
            friend_ids.retain(|&id| id > 2 && id != player_userid);
            if !friend_ids.contains(&BOT_ID) {
                friend_ids.push(BOT_ID);
            }
            if !friend_ids.contains(&tillerino::BOT_ID) {
                friend_ids.push(tillerino::BOT_ID);
            }

            {
                let mut s = state.write().await;
                if let Some(ref mut p) = s.player {
                    p.queue.extend_from_slice(&packets::friends_list(&friend_ids));

                    if friend_rec.friend_id != BOT_ID && friend_rec.friend_id != tillerino::BOT_ID && friend_rec.friend_id != tillerino::OFFICIAL_BOT_ID && friend_rec.friend_id > 2 && friend_rec.friend_id != p.userid {
                        let mut fp = Player::new(friend_rec.friend_name.clone());
                        fp.userid = friend_rec.friend_id;
                        fp.bancho_privs = 1;
                        fp.rank = friend_rec.rank;
                        fp.pp = friend_rec.pp;
                        fp.acc = friend_rec.acc;
                        fp.country = friend_rec.country;
                        fp.ranked_score = friend_rec.ranked_score;
                        fp.total_score = friend_rec.total_score;
                        fp.playcount = friend_rec.playcount;
                        fp.action = 0;
                        fp.info_text = String::new();
                        p.queue.extend_from_slice(&packets::user_presence(&fp));
                        p.queue.extend_from_slice(&packets::user_stats(&fp));
                    }
                }
            }

            let stats_extra = if friend_rec.pp > 0 {
                format!(" | Rank: #{} | PP: {}pp | Acc: {:.2}%", friend_rec.rank, friend_rec.pp, friend_rec.acc)
            } else {
                String::new()
            };
            reply(state, target, &format!("Added {} (ID: {}) to your friends list!{}", friend_rec.friend_name, friend_rec.friend_id, stats_extra)).await;
        }
        "remove" | "rm" | "del" => {
            if args.len() < 2 {
                reply(state, target, "Usage: !friend remove <username or user_id>").await;
                return;
            }
            let user_query = args[1..].join(" ");

            let friends = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                db::get_friends(&db_conn, player_name).unwrap_or_default()
            };

            let is_tillerino_query = user_query.eq_ignore_ascii_case("tillerino") || user_query == "4" || user_query == "2070907";
            let is_banchobot_query = user_query.eq_ignore_ascii_case("banchobot") || user_query == "3";

            let target_id = if is_tillerino_query {
                Some(tillerino::BOT_ID)
            } else if is_banchobot_query {
                Some(BOT_ID)
            } else if let Ok(uid) = user_query.parse::<i32>() {
                Some(uid)
            } else {
                friends.iter().find(|(_, name)| name.eq_ignore_ascii_case(&user_query)).map(|(id, _)| *id)
            };

            let Some(friend_id) = target_id else {
                reply(state, target, &format!("'{}' is not in your friends list.", user_query)).await;
                return;
            };

            if friend_id <= 2 {
                reply(state, target, "Invalid friend ID.").await;
                return;
            }

            let mut friend_ids = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                let _ = db::remove_friend(&db_conn, player_name, friend_id);
                if is_tillerino_query {
                    let _ = db::remove_friend(&db_conn, player_name, tillerino::OFFICIAL_BOT_ID);
                }
                db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
            };
            friend_ids.retain(|&id| id > 2);
            if !friend_ids.contains(&BOT_ID) {
                friend_ids.push(BOT_ID);
            }
            if !friend_ids.contains(&tillerino::BOT_ID) {
                friend_ids.push(tillerino::BOT_ID);
            }

            {
                let mut s = state.write().await;
                if let Some(ref mut p) = s.player {
                    p.queue.extend_from_slice(&packets::friends_list(&friend_ids));
                    if friend_id != BOT_ID && friend_id != tillerino::BOT_ID {
                        p.queue.extend_from_slice(&packets::logout(friend_id));
                    }
                }
            }

            reply(state, target, &format!("Removed friend ID: {} from your friends list.", friend_id)).await;
        }
        "list" | "" => {
            let mut friends = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                db::get_friends(&db_conn, player_name).unwrap_or_default()
            };
            friends.retain(|(id, _)| *id > 2);

            if friends.is_empty() {
                reply(state, target, "Your friends list is currently empty. Use !friend add <username> or !friend sync.").await;
            } else {
                let list_str: Vec<String> = friends
                    .iter()
                    .map(|(id, name)| {
                        if (*id == tillerino::BOT_ID && (name.is_empty() || name == "Friend 4")) || *id == tillerino::OFFICIAL_BOT_ID {
                            "Tillerino (4)".to_string()
                        } else if *id == BOT_ID && (name.is_empty() || name == "Friend 3") {
                            "BanchoBot (3)".to_string()
                        } else if name.is_empty() {
                            id.to_string()
                        } else {
                            format!("{} ({})", name, id)
                        }
                    })
                    .collect();
                reply(state, target, &format!("Friends ({}): {}", friends.len(), list_str.join(", "))).await;
            }
        }
        "sync" => {
            let (uname, pword, http, api_key) = {
                let s = state.read().await;
                match (&s.config.osu_username, &s.config.osu_password) {
                    (Some(u), Some(p)) => (u.clone(), p.clone(), s.http.clone(), s.config.osu_api_key.clone()),
                    _ => {
                        reply(state, target, "Please configure OSU_USERNAME and OSU_PASSWORD in .env or config to sync friends with official Bancho.").await;
                        return;
                    }
                }
            };

            let url = format!("https://osu.ppy.sh/web/osu-getfriends.php?u={}&h={}", uname, pword);
            let resp = match http.get(&url).send().await {
                Ok(r) if r.status() == 200 => r.text().await.unwrap_or_default(),
                _ => {
                    reply(state, target, "Failed to connect to official Bancho friends service.").await;
                    return;
                }
            };

            let mut regular_ids = Vec::new();
            let mut has_tillerino = false;
            let mut has_banchobot = false;
            for line in resp.lines() {
                if let Ok(id) = line.trim().parse::<i32>() {
                    if id <= 2 {
                        // ID <= 2 collides with local player ID 2
                        continue;
                    } else if tillerino::is_tillerino_id(id) {
                        has_tillerino = true;
                    } else if id == BOT_ID {
                        has_banchobot = true;
                    } else {
                        regular_ids.push(id);
                    }
                }
            }

            reply(state, target, &format!("Syncing {} friends from official Bancho... Fetching profiles...", regular_ids.len())).await;

            let sem = std::sync::Arc::new(tokio::sync::Semaphore::new(5));
            let mut join_set = tokio::task::JoinSet::new();

            for id in regular_ids {
                let http = http.clone();
                let key = api_key.clone();
                let sem = sem.clone();
                join_set.spawn(async move {
                    let _permit = sem.acquire().await.ok();
                    let rec = if let Some(ref k) = key {
                        utils::fetch_user_stats_from_api(&http, k, &id.to_string()).await
                    } else {
                        None
                    };
                    (id, rec)
                });
            }

            let mut fetched_results = Vec::new();
            while let Some(res) = join_set.join_next().await {
                if let Ok(item) = res {
                    fetched_results.push(item);
                }
            }

            let (friend_ids, friends) = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;

                let _ = db::remove_friend(&db_conn, player_name, 2);
                let _ = db::remove_friend(&db_conn, player_name, tillerino::OFFICIAL_BOT_ID);

                if has_tillerino {
                    let _ = db::add_friend(&db_conn, player_name, tillerino::BOT_ID, tillerino::BOT_NAME);
                }
                if has_banchobot {
                    let _ = db::add_friend(&db_conn, player_name, BOT_ID, BOT_NAME);
                }

                for (id, rec_opt) in fetched_results {
                    if let Some(rec) = rec_opt {
                        let _ = db::save_friend_stats(&db_conn, player_name, &rec);
                    } else {
                        let _ = db::add_friend(&db_conn, player_name, id, "");
                    }
                }

                let mut ids = db::get_friend_ids(&db_conn, player_name).unwrap_or_default();
                ids.retain(|&id| id > 2);
                if !ids.contains(&BOT_ID) {
                    ids.push(BOT_ID);
                }
                if !ids.contains(&tillerino::BOT_ID) {
                    ids.push(tillerino::BOT_ID);
                }
                let friends = db::get_friend_records(&db_conn, player_name).unwrap_or_default();
                (ids, friends)
            };

            {
                let mut s = state.write().await;
                if let Some(ref mut p) = s.player {
                    let player_id = p.userid;
                    let filtered_ids: Vec<i32> = friend_ids.iter().copied().filter(|&id| id != player_id && id > 2).collect();
                    p.queue.extend_from_slice(&packets::friends_list(&filtered_ids));

                    for f in friends {
                        if f.friend_id <= 2 || f.friend_id == player_id || f.friend_id == BOT_ID || tillerino::is_tillerino_id(f.friend_id) {
                            continue;
                        }
                        let display_name = if f.friend_name.is_empty() { format!("Friend {}", f.friend_id) } else { f.friend_name };
                        let mut fp = Player::new(display_name);
                        fp.userid = f.friend_id;
                        fp.bancho_privs = 1;
                        fp.rank = f.rank;
                        fp.pp = f.pp;
                        fp.acc = f.acc;
                        fp.country = f.country;
                        fp.ranked_score = f.ranked_score;
                        fp.total_score = f.total_score;
                        fp.playcount = f.playcount;
                        fp.action = 0;
                        fp.info_text = String::new();
                        p.queue.extend_from_slice(&packets::user_presence(&fp));
                        p.queue.extend_from_slice(&packets::user_stats(&fp));
                    }
                }
            }

            reply(state, target, &format!("Synced {} friends from official Bancho with updated profiles and stats!", friend_ids.len())).await;
        }
        _ => {
            reply(state, target, "Usage: !friend <add/remove/list/sync> [user]").await;
        }
    }
}

pub async fn recalculate_profile(state: &Arc<RwLock<AppState>>, player_name: &str) -> (usize, i32, f64) {
    let (scores, http, config, db) = {
        let s = state.read().await;
        let conn = s.db.lock().await;
        let sc = db::get_all_scores(&conn, player_name).unwrap_or_default();
        drop(conn);
        (sc, s.http.clone(), s.config.clone(), s.db.clone())
    };

    let mut beatmap_cache: HashMap<String, Option<rosu_pp::Beatmap>> = HashMap::new();
    let mut recalculated_count = 0;

    for score in &scores {
        let Some(score_id) = score.scoreid else {
            continue;
        };

        if !beatmap_cache.contains_key(&score.md5) {
            let mut bmap = {
                let conn = db.lock().await;
                db::get_beatmap_by_md5(&conn, &score.md5).unwrap_or(None)
            };

            let mut content = None;
            if let Some(ref b) = bmap {
                content = utils::get_or_fetch_beatmap_content(&http, &db, &config, b).await;
            }

            if content.is_none() {
                if let Some(songs_dir) = utils::resolve_songs_folder(&config) {
                    if let Some((local_bmap, local_content)) = utils::find_and_parse_local_osu_file(&songs_dir, None, None, Some(&score.md5), None) {
                        let conn = db.lock().await;
                        let _ = db::insert_beatmap(&conn, &local_bmap);
                        let _ = db::update_beatmap_file_content(&conn, &local_bmap.file_md5, &local_content);
                        drop(conn);
                        bmap = Some(local_bmap);
                        content = Some(local_content);
                    }
                }
            }

            if content.is_none() {
                if let Some(ref api_key) = config.osu_api_key {
                    if let Some(api_bmap) = utils::fetch_beatmap_from_api(&http, api_key, &[("h", score.md5.clone())]).await {
                        let conn = db.lock().await;
                        let _ = db::insert_beatmap(&conn, &api_bmap);
                        drop(conn);
                        content = utils::get_or_fetch_beatmap_content(&http, &db, &config, &api_bmap).await;
                    }
                }
            }

            let parsed = content.and_then(|c| rosu_pp::Beatmap::from_bytes(c.as_bytes()).ok());
            beatmap_cache.insert(score.md5.clone(), parsed);
        }

        if let Some(Some(ref parsed_map)) = beatmap_cache.get(&score.md5) {
            let game_mode = match score.mode {
                0 => rosu_pp::model::mode::GameMode::Osu,
                1 => rosu_pp::model::mode::GameMode::Taiko,
                2 => rosu_pp::model::mode::GameMode::Catch,
                3 => rosu_pp::model::mode::GameMode::Mania,
                _ => rosu_pp::model::mode::GameMode::Osu,
            };

            let result = rosu_pp::Performance::new(parsed_map)
                .mode_or_ignore(game_mode)
                .mods(score.mods)
                .n300(score.n300.max(0) as u32)
                .n100(score.n100.max(0) as u32)
                .n50(score.n50.max(0) as u32)
                .n_katu(score.nkatu.max(0) as u32)
                .n_geki(score.ngeki.max(0) as u32)
                .misses(score.nmiss.max(0) as u32)
                .combo(score.max_combo.max(0) as u32)
                .calculate();

            let new_pp = result.pp();
            let new_acc = utils::calculate_accuracy(
                score.mode as u8,
                score.n300,
                score.n100,
                score.n50,
                score.ngeki,
                score.nkatu,
                score.nmiss,
            );

            let conn = db.lock().await;
            let _ = db::update_score_pp_and_acc(&conn, score_id, new_pp, new_acc);
            recalculated_count += 1;
        }
    }

    let (ranked_scores, playcount) = {
        let conn = db.lock().await;
        let sc = db::get_ranked_scores(&conn, player_name).unwrap_or_default();
        let pc = db::get_playcount(&conn, player_name).unwrap_or(0);
        (sc, pc)
    };

    let (new_pp, new_acc, p_mode) = {
        let mut s = state.write().await;
        let filter_mod = s.mode;
        if let Some(ref mut p) = s.player {
            p.calculate_stats(&ranked_scores, filter_mod, playcount);
            (p.pp, p.acc, p.mode)
        } else {
            let mut dummy = Player::new(player_name.to_string());
            dummy.calculate_stats(&ranked_scores, filter_mod, playcount);
            (dummy.pp, dummy.acc, dummy.mode)
        }
    };

    let daily_key = config.osu_daily_api_key.clone();
    let rank = if let Some(ref api_key) = daily_key {
        utils::get_rank_from_daily(&http, api_key, new_pp, p_mode).await
    } else {
        None
    };

    {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            if let Some(r) = rank {
                p.rank = r;
            }
            p.enqueue_stats();
        }
    }

    {
        let conn = db.lock().await;
        let _ = db::update_profile_stats(&conn, player_name, new_pp as f64, new_acc);
    }

    (recalculated_count, new_pp, new_acc)
}

pub async fn handle_recalc(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let (count, new_pp, new_acc) = recalculate_profile(state, player_name).await;
    let msg = format!(
        "Profile recalculation complete! Processed {} score(s). PP: {}pp | Acc: {:.2}%",
        count, new_pp, new_acc
    );
    reply(state, target, &msg).await;
}

pub async fn handle_wipe(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let _ = db::wipe_profile(&db_conn, player_name);
    }

    {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            p.pp = 0;
            p.acc = 0.0;
            p.ranked_score = 0;
            p.total_score = 0;
            p.playcount = 0;
            p.enqueue_stats();
        }
    }

    reply(state, target, "Profile stats and scores have been wiped!").await;
}

pub async fn handle_avatar(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    if args.is_empty() {
        reply(state, target, "Usage: !avatar <image_url_or_path>").await;
        return;
    }

    let url = args.join(" ");
    {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let _ = db::set_avatar(&db_conn, player_name, &url);
    }

    reply(state, target, &format!("Avatar updated to: {}\nRestart your osu! client for the changes to show!", url)).await;
}

pub async fn handle_recent_channel_toggle(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    let new_val = if let Some(arg) = args.first() {
        match arg.to_lowercase().as_str() {
            "on" | "true" | "1" | "enable" => true,
            "off" | "false" | "0" | "disable" => false,
            _ => {
                reply(state, target, "Usage: !recentfeed <on / off / toggle>").await;
                return;
            }
        }
    } else {
        let current = state.read().await.config.enable_recent_channel;
        !current
    };

    {
        let mut s = state.write().await;
        s.config.enable_recent_channel = new_val;
        let db_conn = s.db.lock().await;
        let _ = db::save_config(&db_conn, &s.config);
    }

    if new_val {
        reply(state, target, "Recent score feed (#recent) is now enabled.").await;
    } else {
        reply(state, target, "Recent score feed (#recent) is now disabled.").await;
    }
}

pub async fn handle_config(state: &Arc<RwLock<AppState>>, target: &str) {
    let msg = {
        let s = state.read().await;
        format!(
            "Server Config Summary:\n\
            Version: {} | PP Leaderboard: {}\n\
            Leaderboard Size: {} | Show PP for PB: {}\n\
            Recent Feed (#recent): {} | Auto Update: {}\n\
            osu! Legacy API Key Configured: {}",
            crate::VERSION,
            s.config.pp_leaderboard,
            s.config.amount_of_scores_on_lb,
            s.config.show_pp_for_personal_best,
            s.config.enable_recent_channel,
            s.config.auto_update,
            s.config.osu_api_key.is_some()
        )
    };
    reply(state, target, &msg).await;
}

pub async fn handle_restrictself(state: &Arc<RwLock<AppState>>, player_name: &str, reply_target: &str, args: &[&str]) {
    if let Some(&sub) = args.first() {
        if sub.eq_ignore_ascii_case("off") || sub.eq_ignore_ascii_case("undo") || sub.eq_ignore_ascii_case("lift") {
            handle_unrestrictself(state, player_name, reply_target).await;
            return;
        }
    }

    let is_already_restricted = {
        let s = state.read().await;
        s.player.as_ref().map_or(false, |p| p.is_restricted)
    };

    if is_already_restricted && args.is_empty() {
        handle_unrestrictself(state, player_name, reply_target).await;
        return;
    }

    let reason = if !args.is_empty() {
        let filtered_args: Vec<&str> = if args.first() == Some(&"on") {
            args[1..].to_vec()
        } else {
            args.to_vec()
        };
        if filtered_args.is_empty() {
            None
        } else {
            Some(filtered_args.join(" "))
        }
    } else {
        None
    };

    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        p.is_restricted = true;
        p.bancho_privs = 0;
        p.rank = 0;
        p.pp = 0;

        // 1. In-game notification banner (the iconic yellow toast)
        p.queue.extend_from_slice(&packets::notification(
            "Your account is currently in restricted mode! Please visit the osu! website for more information."
        ));

        // 2. Strip privileges
        p.queue.extend_from_slice(&packets::bancho_privs(0));

        // 3. User stats and presence reflecting restriction (rank 0, pp 0, privs 0)
        p.enqueue_stats();
        p.queue.extend_from_slice(&packets::user_presence(p));

        // 4. BanchoBot direct message with official wording
        let pm_text = if let Some(ref r) = reason {
            format!(
                "Your account is currently in restricted mode! Please visit the osu! website for more information.\nReason: {}\n(Note: This is simulated for funsies. Type !restrictself or !unrestrict to lift restriction.)",
                r
            )
        } else {
            "Your account is currently in restricted mode! Please visit the osu! website for more information.\n(Note: This is simulated for funsies. Type !restrictself or !unrestrict to lift restriction.)".to_string()
        };

        let pm_pkt = packets::send_msg("BanchoBot", &pm_text, &p.name, BOT_ID);
        p.queue.extend_from_slice(&pm_pkt);

        if reply_target.starts_with('#') {
            let chan_text = format!("{} is now in restricted mode.", p.name);
            let chan_pkt = packets::send_msg("BanchoBot", &chan_text, reply_target, BOT_ID);
            p.queue.extend_from_slice(&chan_pkt);
        }
    }
}

pub async fn handle_unrestrictself(state: &Arc<RwLock<AppState>>, player_name: &str, reply_target: &str) {
    let was_restricted = {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            let was = p.is_restricted;
            p.is_restricted = false;
            p.bancho_privs = 63;
            was
        } else {
            false
        }
    };

    if !was_restricted {
        reply(state, reply_target, "You are not currently in restricted mode.").await;
        return;
    }

    // Recalculate true stats from DB and restore global rank if configured
    recalculate_profile(state, player_name).await;

    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        p.bancho_privs = 63;
        p.queue.extend_from_slice(&packets::bancho_privs(63));
        p.queue.extend_from_slice(&packets::notification(
            "Your account restriction has been lifted! Welcome back."
        ));
        p.queue.extend_from_slice(&packets::user_presence(p));

        let pm_text = "Your restriction has been lifted! Your stats and rankings have been restored. Have fun playing!";
        let pm_pkt = packets::send_msg("BanchoBot", pm_text, &p.name, BOT_ID);
        p.queue.extend_from_slice(&pm_pkt);

        if reply_target.starts_with('#') {
            let chan_pkt = packets::send_msg("BanchoBot", pm_text, reply_target, BOT_ID);
            p.queue.extend_from_slice(&chan_pkt);
        }
    }
}

pub async fn handle_roll(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    let max = args.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(100);
    let result = roll_dice(max);
    reply(state, target, &format!("{} rolls {} point(s) (1-{})!", player_name, result, max)).await;
}

pub async fn handle_mode(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    if args.is_empty() {
        reply(state, target, "Usage: !mode <vn / rx / ap>").await;
        return;
    }

    let sub = args[0].to_lowercase();
    let filter_mod = match sub.as_str() {
        "rx" | "relax" => {
            let mut s = state.write().await;
            s.mode = Some(Mods::RELAX);
            s.invalid_mods = Mods::INVALID_RELAX;
            s.mode
        }
        "ap" | "autopilot" => {
            let mut s = state.write().await;
            s.mode = Some(Mods::AUTOPILOT);
            s.invalid_mods = Mods::INVALID_AUTOPILOT;
            s.mode
        }
        "vn" | "vanilla" | "standard" => {
            let mut s = state.write().await;
            s.mode = None;
            s.invalid_mods = Mods::INVALID_STANDARD;
            s.mode
        }
        _ => {
            reply(state, target, "Unknown mode! Valid modes: vn (vanilla), rx (relax), ap (autopilot)").await;
            return;
        }
    };

    let (scores, playcount) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, player_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, player_name).unwrap_or(0);
        (scores, playcount)
    };

    {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            p.calculate_stats(&scores, filter_mod, playcount);
            p.enqueue_stats();
        }
    }

    reply(state, target, &format!("Game mode switched to {}!", sub)).await;
}

pub async fn handle_country(state: &Arc<RwLock<AppState>>, _player_name: &str, target: &str, args: &[&str]) {
    let p_prefix = {
        let s = state.read().await;
        s.config.command_prefix.clone()
    };

    let Some(code_input) = args.first() else {
        let current = {
            let s = state.read().await;
            s.player.as_ref().map(|p| p.country).unwrap_or(0)
        };
        let code_str = utils::country_byte_to_code(current);
        reply(
            state,
            target,
            &format!(
                "Current country: {} (code: {}). Usage: {}country <2-letter ISO code> (e.g. {}country DE, {}country US, {}country AU)",
                code_str, current, p_prefix, p_prefix, p_prefix, p_prefix
            ),
        )
        .await;
        return;
    };

    let trimmed = code_input.trim().to_uppercase();
    let byte = utils::country_code_to_byte(&trimmed);
    if byte == 0 {
        reply(
            state,
            target,
            &format!("Unknown country code '{}'. Please use a standard 2-letter ISO 3166-1 alpha-2 code (e.g. US, DE, AU, JP, GB, FR).", trimmed),
        )
        .await;
        return;
    }

    {
        let mut s = state.write().await;
        s.config.country = Some(trimmed.clone());
        if let Some(ref mut p) = s.player {
            p.country = byte;
            p.queue.extend_from_slice(&packets::user_presence(p));
            p.queue.extend_from_slice(&packets::user_stats(p));
        }
        let db = s.db.lock().await;
        let _ = db::save_profile_country(&db, _player_name, byte);
    }

    let _ = crate::types::config::write_dotenv_country(&trimmed);

    reply(state, target, &format!("Country flag set to {}! Your user panel flag in osu! is updated.", trimmed)).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::beatmap::Beatmap;
    use rusqlite::Connection;

    #[tokio::test]
    async fn test_friend_sync_mapping_and_presence_safety() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        db::add_friend(&conn, "SyncPlayer", 2070907, "LegacyTillerino").unwrap();

        let resp_lines = "2070907\n3\n5555\n";

        let mut synced_count = 0;
        let mut friend_ids = {
            let _ = db::remove_friend(&conn, "SyncPlayer", tillerino::OFFICIAL_BOT_ID);

            for line in resp_lines.lines() {
                if let Ok(id) = line.trim().parse::<i32>() {
                    if tillerino::is_tillerino_id(id) {
                        let _ = db::add_friend(&conn, "SyncPlayer", tillerino::BOT_ID, tillerino::BOT_NAME);
                    } else if id == BOT_ID {
                        let _ = db::add_friend(&conn, "SyncPlayer", BOT_ID, BOT_NAME);
                    } else {
                        let _ = db::add_friend(&conn, "SyncPlayer", id, "");
                    }
                    synced_count += 1;
                }
            }
            db::get_friend_ids(&conn, "SyncPlayer").unwrap_or_default()
        };
        if !friend_ids.contains(&BOT_ID) {
            friend_ids.push(BOT_ID);
        }
        if !friend_ids.contains(&tillerino::BOT_ID) {
            friend_ids.push(tillerino::BOT_ID);
        }

        assert_eq!(synced_count, 3);
        assert!(friend_ids.contains(&3), "Must include BanchoBot");
        assert!(friend_ids.contains(&4), "Must include Tillerino");
        assert!(friend_ids.contains(&5555), "Must include synced friend 5555");
        assert!(!friend_ids.contains(&2070907), "Must not include raw 2070907");

        let friends = db::get_friends(&conn, "SyncPlayer").unwrap();
        assert_eq!(friends.len(), 3);
        assert!(friends.iter().any(|(id, name)| *id == 4 && name == "Tillerino"));
        assert!(friends.iter().any(|(id, name)| *id == 3 && name == "BanchoBot"));
        assert!(friends.iter().any(|(id, _)| *id == 5555));
        assert!(!friends.iter().any(|(id, _)| *id == 2070907));

        let friend_records = db::get_friend_records(&conn, "SyncPlayer").unwrap();
        let mut presence_uids = Vec::new();
        for f in friend_records {
            if f.friend_id <= 2 || f.friend_id == BOT_ID || tillerino::is_tillerino_id(f.friend_id) {
                continue;
            }
            presence_uids.push(f.friend_id);
        }
        assert_eq!(presence_uids, vec![5555]);
    }

    #[tokio::test]
    async fn test_friend_sync_filters_out_peppy_and_local_player_id() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        db::add_friend(&conn, "Hero", 2, "CorruptPeppy").unwrap();

        let resp_lines = "2\n2070907\n3\n12345\n";

        let mut regular_ids = Vec::new();
        let mut has_tillerino = false;
        let mut has_banchobot = false;
        for line in resp_lines.lines() {
            if let Ok(id) = line.trim().parse::<i32>() {
                if id <= 2 {
                    continue;
                } else if tillerino::is_tillerino_id(id) {
                    has_tillerino = true;
                } else if id == BOT_ID {
                    has_banchobot = true;
                } else {
                    regular_ids.push(id);
                }
            }
        }

        assert_eq!(regular_ids, vec![12345]);
        assert!(has_tillerino);
        assert!(has_banchobot);

        let _ = db::remove_friend(&conn, "Hero", 2);
        let _ = db::remove_friend(&conn, "Hero", tillerino::OFFICIAL_BOT_ID);

        if has_tillerino {
            let _ = db::add_friend(&conn, "Hero", tillerino::BOT_ID, tillerino::BOT_NAME);
        }
        if has_banchobot {
            let _ = db::add_friend(&conn, "Hero", BOT_ID, BOT_NAME);
        }
        for id in &regular_ids {
            let _ = db::add_friend(&conn, "Hero", *id, "CoolFriend");
        }

        let mut ids = db::get_friend_ids(&conn, "Hero").unwrap();
        ids.retain(|&id| id > 2);

        assert!(!ids.contains(&2), "Friend IDs must NEVER contain user ID 2!");
        assert!(ids.contains(&3));
        assert!(ids.contains(&4));
        assert!(ids.contains(&12345));

        let records = db::get_friend_records(&conn, "Hero").unwrap();
        let presence_ids: Vec<i32> = records
            .iter()
            .filter(|f| f.friend_id > 2 && f.friend_id != BOT_ID && !tillerino::is_tillerino_id(f.friend_id))
            .map(|f| f.friend_id)
            .collect();
        assert_eq!(presence_ids, vec![12345]);
    }

    #[tokio::test]
    async fn test_db_init_purges_friend_id_2_and_friend_2_profile() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        conn.execute("INSERT INTO profiles (name) VALUES ('Friend 2')", []).unwrap();
        conn.execute("INSERT INTO avatars (player_name) VALUES ('Friend 2')", []).unwrap();
        conn.execute("INSERT INTO friends (player_name, friend_id, friend_name) VALUES ('MyHero', 2, '')", []).unwrap();
        conn.execute("INSERT INTO friends (player_name, friend_id, friend_name) VALUES ('MyHero', 2070907, 'Tillerino')", []).unwrap();

        db::init_db(&conn).unwrap();

        let friends = db::get_friends(&conn, "MyHero").unwrap();
        assert!(friends.is_empty(), "Corrupt friend_id 2 and 2070907 must be purged on init_db");

        let p_count: i32 = conn.query_row("SELECT COUNT(*) FROM profiles WHERE name = 'Friend 2'", [], |r| r.get(0)).unwrap();
        assert_eq!(p_count, 0, "Friend 2 profile must be purged");

        let a_count: i32 = conn.query_row("SELECT COUNT(*) FROM avatars WHERE player_name = 'Friend 2'", [], |r| r.get(0)).unwrap();
        assert_eq!(a_count, 0, "Friend 2 avatar must be purged");
    }

    #[tokio::test]
    async fn test_resolve_target_map_id_prefers_active_md5_over_stale_np() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        // Map A (stale np map)
        let mut map_a = Beatmap::blank();
        map_a.beatmap_id = 1111;
        map_a.file_md5 = "md5_a".to_string();
        map_a.title = "Map A".to_string();
        map_a.version = "Diff A".to_string();
        db::insert_beatmap(&conn, &map_a).unwrap();

        // Map B (active player map)
        let mut map_b = Beatmap::blank();
        map_b.beatmap_id = 2222;
        map_b.file_md5 = "md5_b".to_string();
        map_b.title = "Map B".to_string();
        map_b.version = "Diff B".to_string();
        db::insert_beatmap(&conn, &map_b).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("Player1".to_string());
        player.map_md5 = "md5_b".to_string(); // active map is Map B!
        player.map_id = 0;
        app_state.player = Some(player);
        app_state.last_np_map = Some(map_a); // stale last_np_map is Map A!

        let state = Arc::new(RwLock::new(app_state));

        // When no argument is given, it must resolve Map B (2222), NOT stale Map A (1111)
        let resolved = resolve_target_map_id(&state, &[]).await;
        assert_eq!(resolved, Some(2222));

        // When explicit ID is passed in args, it uses that
        let resolved_explicit = resolve_target_map_id(&state, &["9999"]).await;
        assert_eq!(resolved_explicit, Some(9999));
    }

    #[tokio::test]
    async fn test_handle_set_status_ranks_unindexed_beatmap() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("MyAngelKyrie".to_string());
        player.map_id = 5315861;
        player.map_md5 = "e10adc3949ba59abbe56e057f20f883e".to_string();
        player.info_text = "Kyrie - Test Beatmap (Mapper) [Insane]".to_string();
        app_state.player = Some(player);

        let state = Arc::new(RwLock::new(app_state));

        // Attempt to rank the beatmap ID 5315861 (which is NOT in DB and NOT on API)
        handle_set_status(&state, "MyAngelKyrie", &["5315861"], 1, "Ranked").await;

        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let ranked = db::get_beatmap_by_id(&db_conn, 5315861).unwrap();
        assert!(ranked.is_some(), "Beatmap must be created and saved in local database");
        let b = ranked.unwrap();
        assert_eq!(b.approved, 1);
        assert_eq!(b.artist, "Kyrie");
        assert_eq!(b.title, "Test Beatmap");
        assert_eq!(b.creator, "Mapper");
        assert_eq!(b.version, "Insane");
        assert_eq!(b.file_md5, "e10adc3949ba59abbe56e057f20f883e");

        // Also ensure looking up by MD5 finds the ranked beatmap
        let by_md5 = db::get_beatmap_by_md5(&db_conn, "e10adc3949ba59abbe56e057f20f883e").unwrap();
        assert!(by_md5.is_some());
        assert_eq!(by_md5.unwrap().approved, 1);
    }

    #[tokio::test]
    async fn test_recalculate_command_and_profile_pp() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Alice").unwrap();

        let osu_content = "osu file format v14\n\n[General]\nAudioFilename: a.mp3\nMode: 0\n\n[Metadata]\nTitle:T\nArtist:A\nCreator:C\nVersion:V\n\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n\n[TimingPoints]\n0,500,4,2,0,50,1,0\n\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n256,192,2000,1,0,0:0:0:0:\n256,192,3000,1,0,0:0:0:0:\n256,192,4000,1,0,0:0:0:0:\n";

        let mut bmap = Beatmap::blank();
        bmap.file_md5 = "recalc_map_md5".to_string();
        bmap.beatmap_id = 9991;
        bmap.beatmapset_id = 999;
        bmap.approved = 1;
        bmap.file_content = Some(osu_content.to_string());
        db::insert_beatmap(&conn, &bmap).unwrap();

        let mut score = crate::types::score::Score {
            mode: 0,
            md5: "recalc_map_md5".to_string(),
            name: "Alice".to_string(),
            n300: 4,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 1000000,
            max_combo: 4,
            perfect: true,
            mods: 0,
            time: 1234567,
            acc: Some(100.0),
            pp: Some(9999.0), // Old inflated PP from previous bug
            replay_md5: None,
            scoreid: None,
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        let score_id = db::insert_score(&conn, &score, "ranked").unwrap();
        score.scoreid = Some(score_id);

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("Alice".to_string());
        player.pp = 9999;
        app_state.player = Some(player);

        let state = Arc::new(RwLock::new(app_state));

        // Test calling !recalculate command
        handle_command(&state, "Alice", "Alice", "recalculate", &[]).await;

        // Check that score PP was recalculated (not 9999.0 anymore)
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let score_after = db::get_score_by_id(&db_conn, score_id).unwrap().unwrap();
        assert!(score_after.pp.is_some());
        let recalculated_pp = score_after.pp.unwrap();
        assert!(recalculated_pp < 500.0, "Recalculated PP should be realistic, got {}", recalculated_pp);
        assert!(recalculated_pp >= 0.0);

        // Check that profile was updated
        let player = s.player.as_ref().unwrap();
        assert!(player.pp < 500);
        assert_eq!(player.pp, recalculated_pp.round() as i32);
        drop(db_conn);
        drop(s);

        // Check that BanchoBot sent confirmation packet
        let mut s_write = state.write().await;
        let queue = s_write.player.as_mut().unwrap().clear_queue();
        drop(s_write);
        let packets = packets::split_packets(&queue);
        let has_recalc_msg = packets.iter().any(|pkt| {
            if pkt.id == packets::PacketId::ChoSendMessage as u16 {
                let mut r = packets::PacketReader::new(pkt.payload);
                let _sender = r.read_string().unwrap_or_default();
                let msg = r.read_string().unwrap_or_default();
                msg.contains("Profile recalculation complete! Processed 1 score(s).")
            } else {
                false
            }
        });
        assert!(has_recalc_msg, "BanchoBot must send confirmation message with score count");

        // Also test !recalc alias
        handle_command(&state, "Alice", "Alice", "recalc", &[]).await;
    }

    #[tokio::test]
    async fn test_handle_set_status_ranks_practice_diff_without_corrupting_top_diff() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Player1").unwrap();

        // Top diff (ID: 129891, unranked)
        let mut top_diff = Beatmap::blank();
        top_diff.beatmap_id = 129891;
        top_diff.beatmapset_id = 39804;
        top_diff.file_md5 = "top_diff_md5_123".to_string();
        top_diff.title = "Freedom Dive".to_string();
        top_diff.version = "FOUR DIMENSIONS".to_string();
        top_diff.approved = 0;
        db::insert_beatmap(&conn, &top_diff).unwrap();

        // Practice diff (ID: 0, unranked)
        let mut practice_diff = Beatmap::blank();
        practice_diff.beatmap_id = 0;
        practice_diff.beatmapset_id = 39804;
        practice_diff.file_md5 = "practice_diff_md5_456".to_string();
        practice_diff.title = "Freedom Dive".to_string();
        practice_diff.version = "FOUR DIMENSIONS [Practice]".to_string();
        practice_diff.approved = 0;
        db::insert_beatmap(&conn, &practice_diff).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("Player1".to_string());
        player.map_md5 = "practice_diff_md5_456".to_string();
        player.map_id = 0; // Practice diff has ID 0
        app_state.player = Some(player);

        let state = Arc::new(RwLock::new(app_state));

        // Player ranks their current map without specifying args
        handle_set_status(&state, "Player1", &[], 1, "Ranked").await;

        // Verify: Practice diff is now Ranked, with beatmap_id = 0
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let p_bmap = db::get_beatmap_by_md5(&db_conn, "practice_diff_md5_456").unwrap().unwrap();
        assert_eq!(p_bmap.approved, 1, "Practice diff must be marked ranked");
        assert_eq!(p_bmap.beatmap_id, 0, "Practice diff beatmap_id must NOT become the top diff ID");
        assert_eq!(p_bmap.version, "FOUR DIMENSIONS [Practice]");

        // Verify: Top diff is completely untouched (still approved = 0)
        let t_bmap = db::get_beatmap_by_id(&db_conn, 129891).unwrap().unwrap();
        assert_eq!(t_bmap.approved, 0, "Top diff must remain unranked and untouched!");
        assert_eq!(t_bmap.file_md5, "top_diff_md5_123");
        assert_eq!(t_bmap.version, "FOUR DIMENSIONS");
    }

    #[tokio::test]
    async fn test_handle_set_status_set_ranks_all_diffs_in_mapset() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Player1").unwrap();

        // Diff 1 in set 5555
        let mut diff1 = Beatmap::blank();
        diff1.beatmap_id = 5001;
        diff1.beatmapset_id = 5555;
        diff1.file_md5 = "diff1_md5".to_string();
        diff1.artist = "Xi".to_string();
        diff1.title = "Blue Zenith".to_string();
        diff1.version = "Normal".to_string();
        diff1.approved = 0;
        db::insert_beatmap(&conn, &diff1).unwrap();

        // Diff 2 in set 5555
        let mut diff2 = Beatmap::blank();
        diff2.beatmap_id = 5002;
        diff2.beatmapset_id = 5555;
        diff2.file_md5 = "diff2_md5".to_string();
        diff2.artist = "Xi".to_string();
        diff2.title = "Blue Zenith".to_string();
        diff2.version = "Insane".to_string();
        diff2.approved = 0;
        db::insert_beatmap(&conn, &diff2).unwrap();

        // Diff 3 in set 5555
        let mut diff3 = Beatmap::blank();
        diff3.beatmap_id = 5003;
        diff3.beatmapset_id = 5555;
        diff3.file_md5 = "diff3_md5".to_string();
        diff3.artist = "Xi".to_string();
        diff3.title = "Blue Zenith".to_string();
        diff3.version = "FOUR DIMENSIONS".to_string();
        diff3.approved = 0;
        db::insert_beatmap(&conn, &diff3).unwrap();

        let mut config = crate::types::config::Config::default();
        config.paths.songs = Some("target".to_string());
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("Player1".to_string());
        player.map_md5 = "diff2_md5".to_string(); // active diff is Insane
        player.map_id = 5002;
        app_state.player = Some(player);

        let state = Arc::new(RwLock::new(app_state));

        // 1. Run "!rank set" on the active mapset
        handle_set_status(&state, "Player1", &["set"], 1, "Ranked").await;

        let s = state.read().await;
        let db_conn = s.db.lock().await;

        // All 3 diffs should now be ranked
        let d1 = db::get_beatmap_by_id(&db_conn, 5001).unwrap().unwrap();
        let d2 = db::get_beatmap_by_id(&db_conn, 5002).unwrap().unwrap();
        let d3 = db::get_beatmap_by_id(&db_conn, 5003).unwrap().unwrap();
        assert_eq!(d1.approved, 1);
        assert_eq!(d2.approved, 1);
        assert_eq!(d3.approved, 1);
        drop(db_conn);
        drop(s);

        // BanchoBot reply should confirm set ranking with difficulty count
        let mut s_write = state.write().await;
        let queue = s_write.player.as_mut().unwrap().clear_queue();
        drop(s_write);
        let packets = packets::split_packets(&queue);
        let reply_found = packets.iter().any(|pkt| {
            if pkt.id == packets::PacketId::ChoSendMessage as u16 {
                let mut r = packets::PacketReader::new(pkt.payload);
                let _sender = r.read_string().unwrap_or_default();
                let msg = r.read_string().unwrap_or_default();
                msg.contains("Beatmapset Xi - Blue Zenith (Set ID: 5555) (3 difficulties) is now Ranked!")
            } else {
                false
            }
        });
        assert!(reply_found, "Reply must confirm set ranking for all 3 difficulties");

        // 2. Test "!unrank set 5555" with explicit ID
        handle_set_status(&state, "Player1", &["set", "5555"], 0, "Unranked").await;

        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let d1 = db::get_beatmap_by_id(&db_conn, 5001).unwrap().unwrap();
        let d2 = db::get_beatmap_by_id(&db_conn, 5002).unwrap().unwrap();
        let d3 = db::get_beatmap_by_id(&db_conn, 5003).unwrap().unwrap();
        assert_eq!(d1.approved, 0);
        assert_eq!(d2.approved, 0);
        assert_eq!(d3.approved, 0);
        drop(db_conn);
        drop(s);

        // 3. Test "!rank diff" on active map only ranks that single diff
        handle_set_status(&state, "Player1", &["diff"], 1, "Ranked").await;

        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let d1 = db::get_beatmap_by_id(&db_conn, 5001).unwrap().unwrap();
        let d2 = db::get_beatmap_by_id(&db_conn, 5002).unwrap().unwrap();
        let d3 = db::get_beatmap_by_id(&db_conn, 5003).unwrap().unwrap();
        assert_eq!(d1.approved, 0, "Diff 1 must stay unranked");
        assert_eq!(d2.approved, 1, "Active diff 2 must be ranked");
        assert_eq!(d3.approved, 0, "Diff 3 must stay unranked");
        drop(db_conn);
        drop(s);

        // 4. Test "!status set" reporting mixed statuses
        handle_status(&state, "Player1", &["set"]).await;
        let mut s_write = state.write().await;
        let queue = s_write.player.as_mut().unwrap().clear_queue();
        drop(s_write);
        let packets = packets::split_packets(&queue);
        let status_found = packets.iter().any(|pkt| {
            if pkt.id == packets::PacketId::ChoSendMessage as u16 {
                let mut r = packets::PacketReader::new(pkt.payload);
                let _sender = r.read_string().unwrap_or_default();
                let msg = r.read_string().unwrap_or_default();
                msg.contains("1 Ranked, 2 Unranked")
            } else {
                false
            }
        });
        assert!(status_found, "Status set message must report mixed diff counts");
    }

    #[tokio::test]
    async fn test_restrictself_and_unrestrictself_flow() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "RestrictedUser").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("RestrictedUser".to_string());
        player.rank = 1234;
        player.pp = 500;
        player.bancho_privs = 63;
        app_state.player = Some(player);

        let state = Arc::new(RwLock::new(app_state));

        // 1. Invoke !restrictself with a custom reason
        handle_restrictself(&state, "RestrictedUser", "#osu", &["blatant", "relax", "hacks"]).await;

        {
            let s = state.read().await;
            let p = s.player.as_ref().unwrap();
            assert!(p.is_restricted, "Player must be marked as restricted");
            assert_eq!(p.bancho_privs, 0, "Bancho privileges must be revoked to 0");
            assert_eq!(p.rank, 0, "Rank must be wiped to 0");
            assert_eq!(p.pp, 0, "PP must be wiped to 0");
        }

        // Verify queued packets: notification, bancho_privs(0), user_stats, user_presence, BanchoBot message
        let queue = {
            let mut s = state.write().await;
            s.player.as_mut().unwrap().clear_queue()
        };
        let pkts = packets::split_packets(&queue);
        assert!(pkts.iter().any(|pkt| pkt.id == packets::PacketId::ChoNotification as u16), "Must queue restriction notification");
        assert!(pkts.iter().any(|pkt| pkt.id == packets::PacketId::ChoPrivileges as u16), "Must queue ChoPrivileges packet");
        assert!(pkts.iter().any(|pkt| pkt.id == packets::PacketId::ChoUserStats as u16), "Must queue ChoUserStats packet");
        assert!(pkts.iter().any(|pkt| pkt.id == packets::PacketId::ChoUserPresence as u16), "Must queue ChoUserPresence packet");

        // Verify notification content
        let notif_pkt = pkts.iter().find(|pkt| pkt.id == packets::PacketId::ChoNotification as u16).unwrap();
        let mut notif_reader = packets::PacketReader::new(notif_pkt.payload);
        let notif_text = notif_reader.read_string().unwrap();
        assert!(notif_text.contains("restricted mode"), "Notification must mention restricted mode");

        // Verify BanchoBot message includes the reason
        let msg_pkt = pkts.iter().find(|pkt| {
            if pkt.id == packets::PacketId::ChoSendMessage as u16 {
                let mut r = packets::PacketReader::new(pkt.payload);
                let _sender = r.read_string().unwrap_or_default();
                let msg = r.read_string().unwrap_or_default();
                msg.contains("blatant relax hacks")
            } else {
                false
            }
        });
        assert!(msg_pkt.is_some(), "BanchoBot message must mention the restriction reason");

        // 2. Invoke !restrictself again (toggle off)
        handle_restrictself(&state, "RestrictedUser", "#osu", &[]).await;

        {
            let s = state.read().await;
            let p = s.player.as_ref().unwrap();
            assert!(!p.is_restricted, "Player must no longer be restricted after toggle");
            assert_eq!(p.bancho_privs, 63, "Bancho privileges must be restored");
        }

        // Verify unrestrict notification and packets
        let unrestrict_queue = {
            let mut s = state.write().await;
            s.player.as_mut().unwrap().clear_queue()
        };
        let unrestrict_pkts = packets::split_packets(&unrestrict_queue);
        assert!(unrestrict_pkts.iter().any(|pkt| {
            if pkt.id == packets::PacketId::ChoNotification as u16 {
                let mut r = packets::PacketReader::new(pkt.payload);
                let msg = r.read_string().unwrap_or_default();
                msg.contains("restriction has been lifted")
            } else {
                false
            }
        }), "Must queue restriction lifted notification");

        // 3. Test explicit !unrestrict when not restricted
        handle_unrestrictself(&state, "RestrictedUser", "#osu").await;
        let not_restricted_queue = {
            let mut s = state.write().await;
            s.player.as_mut().unwrap().clear_queue()
        };
        let not_restr_pkts = packets::split_packets(&not_restricted_queue);
        let info_msg = not_restr_pkts.iter().any(|pkt| {
            if pkt.id == packets::PacketId::ChoSendMessage as u16 {
                let mut r = packets::PacketReader::new(pkt.payload);
                let _sender = r.read_string().unwrap_or_default();
                let msg = r.read_string().unwrap_or_default();
                msg.contains("not currently in restricted mode")
            } else {
                false
            }
        });
        assert!(info_msg, "Must inform player they are not currently restricted");
    }
}
