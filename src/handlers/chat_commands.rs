use crate::db;
use crate::packets;
use crate::state::AppState;
use crate::types::beatmap::Beatmap;
use crate::types::mods::Mods;
use crate::types::player::Player;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

pub struct NpInfo {
    pub map_id: Option<i64>,
    pub set_id: Option<i64>,
    pub title_hint: Option<String>,
    pub mods: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct NpBreakdown {
    pub map_id: i64,
    pub artist: String,
    pub title: String,
    pub version: String,
    pub mods_str: String,
    pub stars: f64,
    pub max_combo: u32,
    pub ar: f32,
    pub od: f32,
    pub pp95: f64,
    pub pp98: f64,
    pub pp99: f64,
    pub pp100: f64,
}

// Parses map ID, set ID, title, and mods from /np or client /me actions
pub fn parse_np_message(msg: &str) -> Option<NpInfo> {
    let trimmed = msg.trim();
    let is_action = msg.contains("ACTION is listening to ")
        || msg.contains("ACTION is playing ")
        || msg.contains("ACTION is watching ")
        || msg.contains("is listening to ")
        || msg.contains("is playing ")
        || msg.contains("is watching ");
    let is_direct_np = trimmed == "/np" || trimmed == "!np" || trimmed.starts_with("/np ") || trimmed.starts_with("!np ");

    if !is_action && !is_direct_np {
        return None;
    }

    let mut map_id = None;
    let mut set_id = None;
    let mut title_hint = None;
    let mut mods = None;

    for tag in &["#osu/", "#taiko/", "#fruits/", "#mania/"] {
        if let Some(pos) = msg.find(tag) {
            let after = &msg[pos + tag.len()..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<i64>() {
                map_id = Some(id);
                break;
            }
        }
    }

    if map_id.is_none() {
        if let Some(pos) = msg.find("/b/") {
            let after = &msg[pos + 3..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<i64>() {
                map_id = Some(id);
            }
        }
    }

    if let Some(pos) = msg.find("/beatmapsets/") {
        let after = &msg[pos + "/beatmapsets/".len()..];
        let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(id) = digits.parse::<i64>() {
            set_id = Some(id);
        }
    }

    if set_id.is_none() {
        if let Some(pos) = msg.find("/s/") {
            let after = &msg[pos + 3..];
            let digits: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(id) = digits.parse::<i64>() {
                set_id = Some(id);
            }
        }
    }

    if let Some(start_bracket) = msg.find('[') {
        let inner = &msg[start_bracket + 1..];
        let inner_end = inner.rfind(']').unwrap_or(inner.len());
        let inside = &inner[..inner_end];
        if let Some(space_pos) = inside.find(' ') {
            let title_part = inside[space_pos + 1..].trim();
            if !title_part.is_empty() {
                title_hint = Some(title_part.to_string());
            }
        } else if !inside.starts_with("http") {
            title_hint = Some(inside.trim().to_string());
        }
    }

    if title_hint.is_none() {
        for prefix in &["is listening to ", "is playing ", "is watching "] {
            if let Some(pos) = msg.find(prefix) {
                let after = &msg[pos + prefix.len()..];
                let clean = after.trim_matches(|c| c == '\x01' || c == '[' || c == ']').trim();
                let title_only = if let Some(plus_idx) = clean.rfind(" +") { &clean[..plus_idx] } else { clean };
                if !title_only.is_empty() {
                    title_hint = Some(title_only.to_string());
                }
                break;
            }
        }
    }

    if let Some(plus_pos) = msg.rfind('+') {
        let after = &msg[plus_pos + 1..];
        let mod_str: String = after.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        if !mod_str.is_empty() {
            let parsed = Mods::from_short_str(&mod_str);
            if !parsed.is_empty() {
                mods = Some(parsed.bits());
            }
        }
    }

    Some(NpInfo { map_id, set_id, title_hint, mods })
}

pub fn calculate_np_breakdown(bmap: &Beatmap, content: &str, mods_u32: u32) -> Option<NpBreakdown> {
    let parsed_map = rosu_pp::Beatmap::from_bytes(content.as_bytes()).ok()?;

    let game_mode = match bmap.mode {
        0 => rosu_pp::model::mode::GameMode::Osu,
        1 => rosu_pp::model::mode::GameMode::Taiko,
        2 => rosu_pp::model::mode::GameMode::Catch,
        3 => rosu_pp::model::mode::GameMode::Mania,
        _ => rosu_pp::model::mode::GameMode::Osu,
    };

    let pp95 = rosu_pp::Performance::new(&parsed_map).mode_or_ignore(game_mode).mods(mods_u32).accuracy(95.0).calculate().pp();
    let pp98 = rosu_pp::Performance::new(&parsed_map).mode_or_ignore(game_mode).mods(mods_u32).accuracy(98.0).calculate().pp();
    let pp99 = rosu_pp::Performance::new(&parsed_map).mode_or_ignore(game_mode).mods(mods_u32).accuracy(99.0).calculate().pp();
    let pp100 = rosu_pp::Performance::new(&parsed_map).mode_or_ignore(game_mode).mods(mods_u32).accuracy(100.0).calculate().pp();

    let diff = rosu_pp::Difficulty::new().mods(mods_u32).calculate(&parsed_map);
    let stars = diff.stars();
    let max_combo = diff.max_combo();

    let mut ar = parsed_map.ar;
    let mut od = parsed_map.od;
    if let rosu_pp::any::DifficultyAttributes::Osu(ref osu_diff) = diff {
        ar = osu_diff.ar as f32;
        od = osu_diff.od as f32;
    }

    let mods_obj = Mods::from_bits_truncate(mods_u32);
    let mods_str = if mods_obj.is_empty() { String::new() } else { format!("+{}", mods_obj.short_name()) };

    Some(NpBreakdown {
        map_id: bmap.beatmap_id,
        artist: bmap.artist.clone(),
        title: bmap.title.clone(),
        version: bmap.version.clone(),
        mods_str,
        stars,
        max_combo,
        ar,
        od,
        pp95,
        pp98,
        pp99,
        pp100,
    })
}

pub fn format_np_reply(b: &NpBreakdown) -> String {
    let mods_part = if b.mods_str.is_empty() { String::new() } else { format!(" {}", b.mods_str) };

    let map_link = if b.map_id > 0 { format!("https://osu.ppy.sh/b/{}", b.map_id) } else { "local beatmap".to_string() };

    format!(
        "[{} {} - {} [{}]{}]\n★ {:.2} | Max Combo: {}x | AR: {:.1} OD: {:.1}\n95%: {:.0}pp | 98%: {:.0}pp | 99%: {:.0}pp | 100%: {:.0}pp",
        map_link, b.artist, b.title, b.version, mods_part, b.stars, b.max_combo, b.ar, b.od, b.pp95, b.pp98, b.pp99, b.pp100
    )
}

// Xorshift PRNG seeded by Unix timestamp
fn roll_dice(max: u32) -> u32 {
    let max = if max == 0 { 100 } else { max };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(42);
    let mut x = nanos ^ 0x517cc1b727220a95;
    x ^= x >> 12;
    x ^= x << 25;
    x ^= x >> 27;
    let r = x.wrapping_mul(0x2545f4914f6cdd1d);
    ((r % (max as u64)) + 1) as u32
}

pub async fn handle_chat_message(state: Arc<RwLock<AppState>>, player_name: &str, message: &str, target: &str) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }

    let is_pm = target.eq_ignore_ascii_case("BanchoBot") || !target.starts_with('#');
    let reply_target = if is_pm { player_name.to_string() } else { target.to_string() };

    if let Some(np_info) = parse_np_message(message) {
        handle_np(&state, player_name, &reply_target, np_info).await;
        return;
    }

    let prefix = {
        let s = state.read().await;
        s.config.command_prefix.clone()
    };

    let cmd_text = if let Some(stripped) = trimmed.strip_prefix(&prefix) {
        stripped
    } else if is_pm {
        trimmed.strip_prefix('!').unwrap_or(trimmed)
    } else {
        return;
    };

    let parts: Vec<&str> = cmd_text.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let cmd = parts[0].to_lowercase();
    let args = &parts[1..];

    match cmd.as_str() {
        "help" | "h" => handle_help(&state, &reply_target).await,
        "with" => handle_with(&state, &reply_target, args).await,
        "acc" => handle_acc(&state, &reply_target, args).await,
        "rank" => handle_set_status(&state, &reply_target, args, 1, "Ranked").await,
        "love" => handle_set_status(&state, &reply_target, args, 4, "Loved").await,
        "unrank" => handle_set_status(&state, &reply_target, args, 0, "Unranked").await,
        "status" => handle_status(&state, &reply_target, args).await,
        "friend" | "friends" => handle_friend(&state, player_name, &reply_target, args).await,
        "stats" | "profile" | "p" => handle_stats(&state, player_name, &reply_target, args).await,
        "recent" | "r" | "rs" => handle_recent(&state, player_name, &reply_target).await,
        "tops" | "t" => handle_tops(&state, player_name, &reply_target).await,
        "recalc" => handle_recalc(&state, player_name, &reply_target).await,
        "wipe" => handle_wipe(&state, player_name, &reply_target).await,
        "avatar" => handle_avatar(&state, player_name, &reply_target, args).await,
        "config" => handle_config(&state, &reply_target).await,
        "roll" => handle_roll(&state, player_name, &reply_target, args).await,
        "mode" => handle_mode(&state, player_name, &reply_target, args).await,
        "country" | "flag" => handle_country(&state, player_name, &reply_target, args).await,
        "mybest" | "pb" => handle_mybest(&state, player_name, &reply_target).await,
        "leaderboard" | "lb" => handle_leaderboard(&state, player_name, &reply_target).await,
        _ => {
            if is_pm {
                reply(&state, &reply_target, &format!("Unknown command '{}'. Type !help to see available commands.", cmd)).await;
            }
        }
    }
}

async fn reply(state: &Arc<RwLock<AppState>>, target: &str, text: &str) {
    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        let msg_pkt = packets::send_msg("BanchoBot", text, target, 3);
        p.queue.extend_from_slice(&msg_pkt);
    }
}

async fn handle_np(state: &Arc<RwLock<AppState>>, _player_name: &str, target: &str, np_info: NpInfo) {
    // Resolve beatmap: explicit ID -> player's current map -> last cached /np map
    let (mut bmap, active_mods) = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let mut map = None;
        if let Some(id) = np_info.map_id {
            if let Ok(m) = db::get_beatmap_by_id(&db_conn, id) {
                map = m;
            }
        }
        if map.is_none() {
            if let Some(ref p) = s.player {
                if p.map_id > 0 {
                    if let Ok(m) = db::get_beatmap_by_id(&db_conn, p.map_id as i64) {
                        map = m;
                    }
                }
                if map.is_none() && !p.map_md5.is_empty() {
                    if let Ok(m) = db::get_beatmap_by_md5(&db_conn, &p.map_md5) {
                        map = m;
                    }
                }
            }
        }
        if map.is_none() {
            map = s.last_np_map.clone();
        }

        let mods = np_info.mods.unwrap_or_else(|| s.player.as_ref().map(|p| p.mods).unwrap_or(0));
        (map, mods)
    };

    // Fallback 1: Query official osu! API
    if bmap.is_none() {
        let s = state.read().await;
        if let Some(ref key) = s.config.osu_api_key {
            let mut fetched = None;
            if let Some(id) = np_info.map_id {
                fetched = utils::fetch_beatmap_from_api(&s.http, key, &[("b", id.to_string())]).await;
            }
            if fetched.is_none() {
                if let Some(ref p) = s.player {
                    if p.map_id > 0 {
                        fetched = utils::fetch_beatmap_from_api(&s.http, key, &[("b", p.map_id.to_string())]).await;
                    } else if !p.map_md5.is_empty() {
                        fetched = utils::fetch_beatmap_from_api(&s.http, key, &[("h", p.map_md5.clone())]).await;
                    }
                }
            }
            if fetched.is_none() {
                if let Some(sid) = np_info.set_id {
                    fetched = utils::fetch_beatmap_from_api(&s.http, key, &[("s", sid.to_string())]).await;
                }
            }
            if let Some(b) = fetched {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &b);
                drop(db_conn);
                bmap = Some(b);
            }
        }
    }

    // Fallback 2: Scan local Songs folder
    if bmap.is_none() {
        let s = state.read().await;
        if let Some(songs_dir) = utils::resolve_songs_folder(&s.config) {
            let mid = np_info.map_id.or_else(|| s.player.as_ref().and_then(|p| (p.map_id > 0).then_some(p.map_id as i64)));
            let md5 = s.player.as_ref().map(|p| p.map_md5.as_str()).filter(|m| !m.is_empty());
            let hint = np_info.title_hint.as_deref().or_else(|| s.player.as_ref().map(|p| p.info_text.as_str()).filter(|t| !t.is_empty()));
            let sid = np_info.set_id;

            if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(&songs_dir, sid, mid, md5, hint) {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &local_bmap);
                let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
                drop(db_conn);
                bmap = Some(local_bmap);
            }
        }
    }

    let Some(bmap) = bmap else {
        reply(state, target, "No beatmap selected or found! Select a beatmap in-game first.").await;
        return;
    };

    {
        let mut s = state.write().await;
        s.last_np_map = Some(bmap.clone());
    }

    let content = {
        let s = state.read().await;
        utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, &bmap).await
    };

    let Some(content) = content else {
        reply(state, target, "Failed to load beatmap file (.osu) for calculation.").await;
        return;
    };

    if let Some(breakdown) = calculate_np_breakdown(&bmap, &content, active_mods) {
        let rep = format_np_reply(&breakdown);
        reply(state, target, &rep).await;
    } else {
        reply(state, target, "Could not calculate PP for this beatmap.").await;
    }
}

async fn handle_with(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    if args.is_empty() {
        reply(state, target, "Usage: !with <mods> (e.g. !with HDDT, !with HR, !with NM)").await;
        return;
    }

    let mod_arg = args[0].to_uppercase();
    let new_mods = if mod_arg == "NM" || mod_arg == "NOMOD" { 0 } else { Mods::from_short_str(&mod_arg).bits() };

    let bmap = {
        let s = state.read().await;
        s.last_np_map.clone()
    };

    let Some(bmap) = bmap else {
        reply(state, target, "No beatmap selected! Use /np first.").await;
        return;
    };

    let content = {
        let s = state.read().await;
        utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, &bmap).await
    };

    let Some(content) = content else {
        reply(state, target, "Failed to load beatmap content.").await;
        return;
    };

    if let Some(breakdown) = calculate_np_breakdown(&bmap, &content, new_mods) {
        let rep = format_np_reply(&breakdown);
        reply(state, target, &rep).await;
    } else {
        reply(state, target, "Could not calculate PP with specified mods.").await;
    }
}

async fn handle_acc(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    if args.is_empty() {
        reply(state, target, "Usage: !acc <percentage> (e.g. !acc 97.5)").await;
        return;
    }

    let acc: f64 = match args[0].trim_end_matches('%').parse() {
        Ok(v) if (0.0..=100.0).contains(&v) => v,
        _ => {
            reply(state, target, "Invalid accuracy value. Must be between 0 and 100 (e.g. !acc 98.2)").await;
            return;
        }
    };

    let (bmap, active_mods) = {
        let s = state.read().await;
        let mods = s.player.as_ref().map(|p| p.mods).unwrap_or(0);
        (s.last_np_map.clone(), mods)
    };

    let Some(bmap) = bmap else {
        reply(state, target, "No beatmap selected! Use /np first.").await;
        return;
    };

    let content = {
        let s = state.read().await;
        utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, &bmap).await
    };

    let Some(content) = content else {
        reply(state, target, "Failed to load beatmap content.").await;
        return;
    };

    if let Ok(parsed_map) = rosu_pp::Beatmap::from_bytes(content.as_bytes()) {
        let game_mode = match bmap.mode {
            0 => rosu_pp::model::mode::GameMode::Osu,
            1 => rosu_pp::model::mode::GameMode::Taiko,
            2 => rosu_pp::model::mode::GameMode::Catch,
            3 => rosu_pp::model::mode::GameMode::Mania,
            _ => rosu_pp::model::mode::GameMode::Osu,
        };

        let result = rosu_pp::Performance::new(&parsed_map).mode_or_ignore(game_mode).mods(active_mods).accuracy(acc).calculate();

        let mods_obj = Mods::from_bits_truncate(active_mods);
        let mods_str = if mods_obj.is_empty() { String::new() } else { format!(" +{}", mods_obj.short_name()) };

        let rep = format!(
            "[https://osu.ppy.sh/b/{} {} - {} [{}]{}] | {:.2}%: {:.0}pp",
            bmap.beatmap_id,
            bmap.artist,
            bmap.title,
            bmap.version,
            mods_str,
            acc,
            result.pp()
        );
        reply(state, target, &rep).await;
    } else {
        reply(state, target, "Could not calculate PP.").await;
    }
}

async fn handle_set_status(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str], status: i32, status_name: &str) {
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

async fn handle_status(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
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

async fn handle_friend(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
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
            // Sync friend IDs from official Bancho via osu-getfriends.php
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

async fn handle_help(state: &Arc<RwLock<AppState>>, target: &str) {
    let p = {
        let s = state.read().await;
        s.config.command_prefix.clone()
    };

    let msg = format!(
        "BanchoBot Commands:\n\
        /np : Tillerino PP calculations for current song\n\
        {p}with <mods> : PP calculation with custom mods (e.g. {p}with HDDT)\n\
        {p}acc <val> : PP for specific accuracy (e.g. {p}acc 98.5)\n\
        {p}status / {p}rank / {p}love / {p}unrank [id] : Manage beatmap status\n\
        {p}recent / {p}r : Show your recent play\n\
        {p}tops / {p}t : Show top 5 plays\n\
        {p}stats / {p}profile : Show player stats\n\
        {p}mybest / {p}pb : Show your best score on the current map\n\
        {p}leaderboard / {p}lb : Show top scores on current map\n\
        {p}friend <add/remove/list/sync> : Manage friends\n\
        {p}mode <vn/rx/ap> : Switch game mode\n\
        {p}country / {p}flag <code> : Set country flag (e.g. {p}country DE, {p}country US, {p}country AU)\n\
        {p}roll [max] : Roll a random number (default 100)\n\
        {p}recalc : Recalculate all profile stats\n\
        {p}wipe : Wipe profile stats\n\
        {p}avatar <url or path> : Change avatar\n\
        {p}config : Server settings summary"
    );

    reply(state, target, &msg).await;
}

async fn handle_stats(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, _args: &[&str]) {
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

async fn handle_recent(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
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

async fn handle_tops(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
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

async fn handle_recalc(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
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

async fn handle_wipe(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
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

async fn handle_avatar(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
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

async fn handle_config(state: &Arc<RwLock<AppState>>, target: &str) {
    let msg = {
        let s = state.read().await;
        format!(
            "Server Config Summary:\n\
            Version: 1.6.3 | PP Leaderboard: {}\n\
            Leaderboard Size: {} | Show PP for PB: {}\n\
            Auto Update: {} | osu! API Key Configured: {}",
            s.config.pp_leaderboard,
            s.config.amount_of_scores_on_lb,
            s.config.show_pp_for_personal_best,
            s.config.auto_update,
            s.config.osu_api_key.is_some()
        )
    };
    reply(state, target, &msg).await;
}

async fn handle_roll(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    let max = args.first().and_then(|s| s.parse::<u32>().ok()).unwrap_or(100);
    let result = roll_dice(max);
    reply(state, target, &format!("{} rolls {} point(s) (1-{})!", player_name, result, max)).await;
}

async fn handle_mode(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
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

async fn handle_mybest(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let (map_md5, map_title, mode) = {
        let s = state.read().await;
        let mut md5 = String::new();
        let mut title = String::new();
        let mode = s.player.as_ref().map(|p| p.mode).unwrap_or(0);

        if let Some(ref b) = s.last_np_map {
            md5 = b.file_md5.clone();
            title = format!("{} - {} [{}]", b.artist, b.title, b.version);
        } else if let Some(ref p) = s.player {
            md5 = p.map_md5.clone();
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

async fn handle_leaderboard(state: &Arc<RwLock<AppState>>, _player_name: &str, target: &str) {
    let (map_md5, map_title, mode, player_name) = {
        let s = state.read().await;
        let mut md5 = String::new();
        let mut title = String::new();
        let mode = s.player.as_ref().map(|p| p.mode).unwrap_or(0);
        let name = s.player.as_ref().map(|p| p.name.clone()).unwrap_or_default();

        if let Some(ref b) = s.last_np_map {
            md5 = b.file_md5.clone();
            title = format!("{} - {} [{}]", b.artist, b.title, b.version);
        } else if let Some(ref p) = s.player {
            md5 = p.map_md5.clone();
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

async fn handle_country(state: &Arc<RwLock<AppState>>, _player_name: &str, target: &str, args: &[&str]) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_np_message_action() {
        let msg = "\x01ACTION is listening to [https://osu.ppy.sh/b/123456 Artist - Title [Insane]]\x01";
        let info = parse_np_message(msg).unwrap();
        assert_eq!(info.map_id, Some(123456));
        assert_eq!(info.mods, None);

        let msg_playing = "\x01ACTION is playing [https://osu.ppy.sh/b/98765 Artist - Title [Extra]] <+HDDT>\x01";
        let info_playing = parse_np_message(msg_playing).unwrap();
        assert_eq!(info_playing.map_id, Some(98765));
        assert_eq!(info_playing.mods, Some((Mods::HIDDEN | Mods::DOUBLETIME).bits()));
    }

    #[test]
    fn test_parse_np_message_cuttingedge() {
        let msg = "\x01ACTION is listening to [https://osu.ppy.sh/beatmapsets/1222729#osu/2543274 VINXIS - Sidetracked Day GAMMA]\x01";
        let info = parse_np_message(msg).unwrap();
        assert_eq!(info.map_id, Some(2543274));
        assert_eq!(info.set_id, Some(1222729));
        assert_eq!(info.title_hint.as_deref(), Some("VINXIS - Sidetracked Day GAMMA"));
    }

    #[test]
    fn test_parse_np_message_plain_action() {
        let msg = "*w is listening to VINXIS - Sidetracked Day GAMMA";
        let info = parse_np_message(msg).unwrap();
        assert_eq!(info.map_id, None);
        assert_eq!(info.title_hint.as_deref(), Some("VINXIS - Sidetracked Day GAMMA"));
    }

    #[test]
    fn test_parse_np_message_direct() {
        assert!(parse_np_message("/np").is_some());
        assert!(parse_np_message("!np").is_some());
        assert!(parse_np_message("hello world").is_none());
    }

    #[test]
    fn test_format_np_reply() {
        let b = NpBreakdown {
            map_id: 12345,
            artist: "xi".to_string(),
            title: "Freedom Dive".to_string(),
            version: "FOUR DIMENSIONS".to_string(),
            mods_str: "+HDHR".to_string(),
            stars: 7.89,
            max_combo: 2385,
            ar: 10.0,
            od: 10.0,
            pp95: 550.0,
            pp98: 620.0,
            pp99: 680.0,
            pp100: 750.0,
        };
        let formatted = format_np_reply(&b);
        assert!(formatted.contains("https://osu.ppy.sh/b/12345"));
        assert!(formatted.contains("xi - Freedom Dive [FOUR DIMENSIONS] +HDHR"));
        assert!(formatted.contains("★ 7.89 | Max Combo: 2385x | AR: 10.0 OD: 10.0"));
        assert!(formatted.contains("95%: 550pp | 98%: 620pp | 99%: 680pp | 100%: 750pp"));
    }

    #[test]
    fn test_roll_dice() {
        for _ in 0..10 {
            let r = roll_dice(50);
            assert!(r >= 1 && r <= 50);
        }
    }

    #[test]
    fn test_calculate_np_breakdown_with_sample_map() {
        let osu_content = "osu file format v14\n[General]\nMode: 0\n[Difficulty]\nHPDrainRate:5\nCircleSize:5\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n[TimingPoints]\n0,500,4,1,0,100,1,0\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n256,192,2000,1,0,0:0:0:0:\n";
        let mut bmap = Beatmap::blank();
        bmap.beatmap_id = 999;
        bmap.artist = "Artist".to_string();
        bmap.title = "Title".to_string();
        bmap.version = "Normal".to_string();

        let breakdown = calculate_np_breakdown(&bmap, osu_content, 0).unwrap();
        assert_eq!(breakdown.map_id, 999);
        assert_eq!(breakdown.max_combo, 2);
        assert!(breakdown.pp100 > 0.0);
        assert!(breakdown.pp95 <= breakdown.pp98);
        assert!(breakdown.pp98 <= breakdown.pp99);
        assert!(breakdown.pp99 <= breakdown.pp100);

        let breakdown_hdhr = calculate_np_breakdown(&bmap, osu_content, (Mods::HIDDEN | Mods::HARDROCK).bits()).unwrap();
        assert_eq!(breakdown_hdhr.mods_str, "+HDHR");
        assert!(breakdown_hdhr.ar > breakdown.ar);
    }

    #[tokio::test]
    async fn test_chat_message_help_and_mode_and_status() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "PlayerTest").unwrap();

        let mut bmap = Beatmap::blank();
        bmap.file_md5 = "map_md5_test".to_string();
        bmap.beatmap_id = 5555;
        bmap.approved = 0;
        db::insert_beatmap(&conn, &bmap).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("PlayerTest".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        // Test !help in #osu
        handle_chat_message(shared_state.clone(), "PlayerTest", "!help", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let packets = packets::split_packets(&q);
            assert!(!packets.is_empty());
            assert_eq!(packets[0].id, packets::PacketId::ChoSendMessage as u16);
        }

        // Test !mode rx
        handle_chat_message(shared_state.clone(), "PlayerTest", "!mode rx", "#osu").await;
        {
            let s = shared_state.read().await;
            assert_eq!(s.mode, Some(Mods::RELAX));
        }

        // Test !rank 5555
        handle_chat_message(shared_state.clone(), "PlayerTest", "!rank 5555", "#osu").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let loaded = db::get_beatmap_by_id(&db_conn, 5555).unwrap().unwrap();
            assert_eq!(loaded.approved, 1);
        }

        // Test !friend add with numeric ID
        handle_chat_message(shared_state.clone(), "PlayerTest", "!friend add 9988", "BanchoBot").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friend_ids(&db_conn, "PlayerTest").unwrap();
            assert!(friends.contains(&9988));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!country DE", "#osu").await;
        {
            let s = shared_state.read().await;
            assert_eq!(s.player.as_ref().unwrap().country, 56);
        }
    }
}
