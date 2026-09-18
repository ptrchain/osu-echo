use std::sync::Arc;
use tokio::sync::RwLock;

use crate::db;
use crate::packets;
use crate::state::AppState;
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
        "recalc" => handle_recalc(state, player_name, reply_target).await,
        "wipe" => handle_wipe(state, player_name, reply_target).await,
        "avatar" => handle_avatar(state, player_name, reply_target, args).await,
        "recentfeed" | "recentchannel" => handle_recent_channel_toggle(state, reply_target, args).await,
        "config" => handle_config(state, reply_target).await,
        "roll" => handle_roll(state, player_name, reply_target, args).await,
        "mode" => handle_mode(state, player_name, reply_target, args).await,
        "country" | "flag" => handle_country(state, player_name, reply_target, args).await,
        "mybest" | "pb" => handle_mybest(state, player_name, reply_target).await,
        "leaderboard" | "lb" => handle_leaderboard(state, player_name, reply_target).await,
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
        {p}status / {p}rank / {p}love / {p}unrank [id] : Manage beatmap status\n\
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
        {p}recalc : Recalculate all profile stats\n\
        {p}wipe : Wipe profile stats\n\
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
        let mut bmaps = Vec::new();
        for sc in scs.iter().take(5) {
            let b = db::get_beatmap_by_md5(&db_conn, &sc.md5).unwrap_or(None);
            bmaps.push(b);
        }
        (scs, bmaps)
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

pub async fn handle_set_status(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str], status: i32, status_name: &str) {
    let map_id = if let Some(first) = args.first() {
        first.parse::<i64>().ok()
    } else {
        let s = state.read().await;
        s.last_np_map.as_ref().map(|b| b.beatmap_id).or_else(|| s.player.as_ref().map(|p| p.map_id as i64))
    };

    let Some(map_id) = map_id else {
        reply(state, target, &format!("Usage: !{} [beatmap_id], or select a map first.", status_name.to_lowercase())).await;
        return;
    };

    let (updated, bmap) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let u = db::set_beatmap_status(&db_conn, map_id, status).unwrap_or(0);
        let b = if u > 0 { db::get_beatmap_by_id(&db_conn, map_id).ok().flatten() } else { None };
        (u, b)
    };

    if updated > 0 {
        if let Some(b) = bmap {
            let mut s = state.write().await;
            if let Some(ref mut np) = s.last_np_map {
                if np.beatmap_id == map_id {
                    np.approved = status;
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
    let map_id = if let Some(first) = args.first() {
        first.parse::<i64>().ok()
    } else {
        let s = state.read().await;
        s.last_np_map.as_ref().map(|b| b.beatmap_id).or_else(|| s.player.as_ref().map(|p| p.map_id as i64))
    };

    let Some(map_id) = map_id else {
        reply(state, target, "Usage: !status [beatmap_id], or select a map first.").await;
        return;
    };

    let beatmap = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        db::get_beatmap_by_id(&db_conn, map_id).ok().flatten()
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

            let mut fetched_friend: Option<db::FriendRecord> = None;
            if let Some(ref key) = api_key {
                fetched_friend = utils::fetch_user_stats_from_api(&http, key, &user_query).await;
            }

            let friend_rec = if let Some(rec) = fetched_friend {
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

            let mut friend_ids = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                let _ = db::save_friend_stats(&db_conn, player_name, &friend_rec);
                db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
            };
            if friend_ids.is_empty() {
                friend_ids.push(0);
            }

            {
                let mut s = state.write().await;
                if let Some(ref mut p) = s.player {
                    p.queue.extend_from_slice(&packets::friends_list(&friend_ids));

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

            let mut target_id: Option<i32> = user_query.parse().ok();
            if target_id.is_none() {
                for (id, name) in &friends {
                    if name.eq_ignore_ascii_case(&user_query) {
                        target_id = Some(*id);
                        break;
                    }
                }
            }

            let Some(friend_id) = target_id else {
                reply(state, target, &format!("'{}' is not in your friends list.", user_query)).await;
                return;
            };

            let mut friend_ids = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                let _ = db::remove_friend(&db_conn, player_name, friend_id);
                db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
            };
            if friend_ids.is_empty() {
                friend_ids.push(0);
            }

            {
                let mut s = state.write().await;
                if let Some(ref mut p) = s.player {
                    p.queue.extend_from_slice(&packets::friends_list(&friend_ids));
                    p.queue.extend_from_slice(&packets::logout(friend_id));
                }
            }

            reply(state, target, &format!("Removed friend ID: {} from your friends list.", friend_id)).await;
        }
        "list" | "" => {
            let friends = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                db::get_friends(&db_conn, player_name).unwrap_or_default()
            };

            if friends.is_empty() {
                reply(state, target, "Your friends list is currently empty. Use !friend add <username> or !friend sync.").await;
            } else {
                let list_str: Vec<String> =
                    friends.iter().map(|(id, name)| if name.is_empty() { id.to_string() } else { format!("{} ({})", name, id) }).collect();
                reply(state, target, &format!("Friends ({}): {}", friends.len(), list_str.join(", "))).await;
            }
        }
        "sync" => {
            let (uname, pword, http) = {
                let s = state.read().await;
                match (&s.config.osu_username, &s.config.osu_password) {
                    (Some(u), Some(p)) => (u.clone(), p.clone(), s.http.clone()),
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

            let mut synced_count = 0;
            let mut friend_ids = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                for line in resp.lines() {
                    if let Ok(id) = line.trim().parse::<i32>() {
                        let _ = db::add_friend(&db_conn, player_name, id, "");
                        synced_count += 1;
                    }
                }
                db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
            };
            if friend_ids.is_empty() {
                friend_ids.push(0);
            }

            let friends = {
                let s = state.read().await;
                let db_conn = s.db.lock().await;
                db::get_friend_records(&db_conn, player_name).unwrap_or_default()
            };

            {
                let mut s = state.write().await;
                if let Some(ref mut p) = s.player {
                    p.queue.extend_from_slice(&packets::friends_list(&friend_ids));

                    for f in friends {
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

            reply(state, target, &format!("Synced {} friends from official Bancho server!", synced_count)).await;
        }
        _ => {
            reply(state, target, "Usage: !friend <add/remove/list/sync> [user]").await;
        }
    }
}

pub async fn handle_recalc(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let (scores, playcount, filter_mod) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, player_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, player_name).unwrap_or(0);
        let filter_mod = s.mode;
        (scores, playcount, filter_mod)
    };

    let (pp, acc) = {
        let mut s = state.write().await;
        if let Some(ref mut p) = s.player {
            p.calculate_stats(&scores, filter_mod, playcount);
            let pp = p.pp;
            let acc = p.acc;
            p.enqueue_stats();
            (pp, acc)
        } else {
            (0, 0.0)
        }
    };

    {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let _ = db::update_profile_stats(&db_conn, player_name, pp as f64, acc);
    }

    reply(state, target, "Profile recalculation complete! Stats have been updated.").await;
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
    }

    let _ = crate::types::config::write_dotenv_country(&trimmed);

    reply(state, target, &format!("Country flag set to {}! Your user panel flag in osu! is updated.", trimmed)).await;
}
