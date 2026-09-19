use crate::db;
use crate::packets;
use crate::state::AppState;
use crate::types::beatmap::Beatmap;
use crate::types::mods::Mods;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const BOT_ID: i32 = 4;
pub const OFFICIAL_BOT_ID: i32 = 2070907;
pub const BOT_NAME: &str = "Tillerino";

pub fn is_tillerino_id(id: i32) -> bool {
    id == BOT_ID || id == OFFICIAL_BOT_ID
}

#[derive(Debug, Clone, PartialEq)]
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

pub async fn reply(state: &Arc<RwLock<AppState>>, target: &str, text: &str) {
    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        let msg_pkt = packets::send_msg(BOT_NAME, text, target, BOT_ID);
        p.queue.extend_from_slice(&msg_pkt);
    }
}

pub fn parse_np_message(msg: &str) -> Option<NpInfo> {
    let trimmed = msg.trim();
    let is_action = msg.contains("ACTION is listening to ")
        || msg.contains("ACTION is playing ")
        || msg.contains("ACTION is watching ")
        || msg.contains("ACTION is editing ")
        || msg.contains("is listening to ")
        || msg.contains("is playing ")
        || msg.contains("is watching ")
        || msg.contains("is editing ");
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

    if map_id.is_none() {
        if let Some(pos) = msg.find("/beatmaps/") {
            let after = &msg[pos + 10..];
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
            let rest = &after[digits.len()..];
            if rest.starts_with('/') && map_id.is_none() {
                let diff_digits: String = rest[1..].chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(did) = diff_digits.parse::<i64>() {
                    map_id = Some(did);
                }
            }
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
        for prefix in &["is listening to ", "is playing ", "is watching ", "is editing "] {
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

    if let Some(plus_pos) = msg.rfind(" +") {
        let after = &msg[plus_pos + 2..];
        let mod_str: String = after.chars().take_while(|c| c.is_ascii_alphabetic()).collect();
        if !mod_str.is_empty() {
            let parsed = Mods::from_short_str(&mod_str);
            if !parsed.is_empty() {
                mods = Some(parsed.bits());
            }
        }
    } else if let Some(plus_pos) = msg.rfind('+') {
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
        od = osu_diff.od() as f32;
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

pub async fn handle_command(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, cmd: &str, args: &[&str]) {
    match cmd {
        "r" | "recommend" => handle_recommend(state, player_name, target, args).await,
        "with" => handle_with(state, target, args).await,
        "acc" => handle_acc(state, target, args).await,
        "help" | "h" | "faq" | "info" => handle_help(state, target).await,
        "np" => {
            let np_info = NpInfo { map_id: None, set_id: None, title_hint: None, mods: None };
            handle_np(state, player_name, target, np_info).await;
        }
        _ => {
            reply(state, target, &format!("Unknown command '{}'. Type !help to see available Tillerino commands.", cmd)).await;
        }
    }
}

pub async fn handle_np(state: &Arc<RwLock<AppState>>, _player_name: &str, target: &str, np_info: NpInfo) {
    let (player_map_id, player_map_md5, player_info_text, active_mods) = {
        let s = state.read().await;
        let p_id = s.player.as_ref().and_then(|p| (p.map_id > 0).then_some(p.map_id as i64));
        let p_md5 = s.player.as_ref().map(|p| p.map_md5.clone()).filter(|m| !m.is_empty());
        let p_text = s.player.as_ref().map(|p| p.info_text.clone()).filter(|t| !t.is_empty());
        let mods = np_info.mods.unwrap_or_else(|| s.player.as_ref().map(|p| p.mods).unwrap_or(0));
        (p_id, p_md5, p_text, mods)
    };

    let target_map_id = np_info.map_id.or(player_map_id);
    let target_md5 = if np_info.map_id.is_none() || np_info.map_id == player_map_id {
        player_map_md5.clone()
    } else {
        None
    };
    let target_set_id = np_info.set_id;
    let target_hint = if let Some(ref th) = np_info.title_hint {
        if th.contains('[') && th.contains(']') {
            Some(th.clone())
        } else if let Some(ref pt) = player_info_text {
            if pt.contains('[') && pt.contains(']') {
                Some(pt.clone())
            } else {
                Some(th.clone())
            }
        } else {
            Some(th.clone())
        }
    } else {
        player_info_text.clone()
    };

    let mut bmap: Option<Beatmap> = None;

    {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        if let Some(id) = target_map_id {
            if let Ok(Some(m)) = db::get_beatmap_by_id(&db_conn, id) {
                bmap = Some(m);
            }
        }
        if bmap.is_none() {
            if let Some(ref md5) = target_md5 {
                if let Ok(Some(m)) = db::get_beatmap_by_md5(&db_conn, md5) {
                    bmap = Some(m);
                }
            }
        }
        if bmap.is_none() {
            if let Some(ref last) = s.last_np_map {
                let set_matches = target_set_id.is_some() && Some(last.beatmapset_id) == target_set_id;
                let id_matches = target_map_id.is_some() && target_map_id == Some(last.beatmap_id);
                let md5_matches = target_md5.is_some() && target_md5.as_deref() == Some(&last.file_md5);
                let no_filter = np_info.map_id.is_none() && np_info.set_id.is_none() && np_info.title_hint.is_none();
                if set_matches || id_matches || md5_matches || no_filter {
                    bmap = Some(last.clone());
                }
            }
        }
    }

    if bmap.is_none() {
        let s = state.read().await;
        if let Some(ref key) = s.config.osu_api_key {
            let mut fetched = None;
            if let Some(id) = target_map_id {
                fetched = utils::fetch_beatmap_from_api(&s.http, key, &[("b", id.to_string())]).await;
            }
            if fetched.is_none() {
                if let Some(ref md5) = target_md5 {
                    fetched = utils::fetch_beatmap_from_api(&s.http, key, &[("h", md5.clone())]).await;
                }
            }
            if fetched.is_none() {
                if let Some(sid) = target_set_id {
                    fetched = utils::fetch_beatmap_from_api_with_hint(&s.http, key, &[("s", sid.to_string())], target_hint.as_deref()).await;
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

    if bmap.is_none() {
        let s = state.read().await;
        if let Some(songs_dir) = utils::resolve_songs_folder(&s.config) {
            if let Some((local_bmap, content)) = utils::find_and_parse_local_osu_file(
                &songs_dir,
                target_set_id,
                target_map_id,
                target_md5.as_deref(),
                target_hint.as_deref(),
            ) {
                let db_conn = s.db.lock().await;
                let _ = db::insert_beatmap(&db_conn, &local_bmap);
                let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
                drop(db_conn);
                bmap = Some(local_bmap);
            }
        }
    }

    if bmap.is_none() {
        let s = state.read().await;
        if np_info.map_id.is_none() && np_info.set_id.is_none() && np_info.title_hint.is_none() {
            if let Some(ref last) = s.last_np_map {
                if target_md5.is_none() || target_md5.as_deref() == Some(&last.file_md5) {
                    bmap = Some(last.clone());
                }
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

pub async fn handle_with(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
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
        reply(state, target, "No beatmap selected! Use /np or !r first.").await;
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

pub async fn handle_acc(state: &Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
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
        reply(state, target, "No beatmap selected! Use /np or !r first.").await;
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

pub fn scan_songs_folder_for_candidates(songs_dir: &std::path::Path, mode: i32, max_count: usize) -> Vec<(Beatmap, String)> {
    let mut results = Vec::new();
    if let Ok(entries) = std::fs::read_dir(songs_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Ok(files) = std::fs::read_dir(&path) {
                    for f in files.flatten() {
                        let fp = f.path();
                        if fp.extension().and_then(|e| e.to_str()) == Some("osu") {
                            if let Ok(content) = std::fs::read_to_string(&fp) {
                                if let Some(bmap) = utils::parse_osu_file_to_beatmap(&content, None, None) {
                                    if bmap.mode == mode || mode == 0 {
                                        results.push((bmap, content));
                                        if results.len() >= max_count {
                                            return results;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    results
}

pub async fn handle_recommend(state: &Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    let mut filter_mods = 0u32;
    let mut filter_nomod = false;

    for arg in args {
        let upper = arg.to_uppercase();
        if upper == "NM" || upper == "NOMOD" {
            filter_nomod = true;
        } else if upper == "DT" || upper == "DOUBLETIME" {
            filter_mods |= Mods::DOUBLETIME.bits();
        } else if upper == "HR" || upper == "HARDROCK" {
            filter_mods |= Mods::HARDROCK.bits();
        } else if upper == "HD" || upper == "HIDDEN" {
            filter_mods |= Mods::HIDDEN.bits();
        } else if upper == "FL" || upper == "FLASHLIGHT" {
            filter_mods |= Mods::FLASHLIGHT.bits();
        } else if upper == "HDDT" {
            filter_mods |= (Mods::HIDDEN | Mods::DOUBLETIME).bits();
        } else if upper == "HDHR" {
            filter_mods |= (Mods::HIDDEN | Mods::HARDROCK).bits();
        }
    }

    let (mode, target_pp) = {
        let s = state.read().await;
        let mode: i32 = s.player.as_ref().map(|p| p.mode as i32).unwrap_or(0);
        let db_conn = s.db.lock().await;
        let ranked_scores = db::get_ranked_scores(&db_conn, player_name).unwrap_or_default();
        let scores_for_mode: Vec<_> = ranked_scores.iter().filter(|sc| sc.mode == mode && sc.pp.unwrap_or(0.0) > 0.0).collect();

        let target = if !scores_for_mode.is_empty() {
            let count = scores_for_mode.len().min(5);
            let sum: f64 = scores_for_mode.iter().take(count).map(|sc| sc.pp.unwrap_or(0.0)).sum();
            sum / (count as f64)
        } else if let Some(ref p) = s.player {
            if p.pp > 0 {
                (p.pp as f64) / 10.0
            } else {
                100.0
            }
        } else {
            100.0
        };

        (mode, target.max(20.0))
    };

    let mut candidate_maps = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        db::get_candidate_beatmaps(&db_conn, mode).unwrap_or_default()
    };

    if candidate_maps.len() < 5 {
        let songs_opt = {
            let s = state.read().await;
            utils::resolve_songs_folder(&s.config)
        };
        if let Some(songs_dir) = songs_opt {
            let scanned = scan_songs_folder_for_candidates(&songs_dir, mode, 25);
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            for (bmap, content) in scanned {
                let _ = db::insert_beatmap(&db_conn, &bmap);
                let _ = db::update_beatmap_file_content(&db_conn, &bmap.file_md5, &content);
                if !candidate_maps.iter().any(|m| m.file_md5 == bmap.file_md5) {
                    candidate_maps.push(bmap);
                }
            }
        }
    }

    if candidate_maps.is_empty() {
        reply(
            state,
            target,
            "No beatmaps available to recommend! Play or select songs in-game, or ensure your osu! Songs folder is accessible.",
        )
        .await;
        return;
    }

    let recent_md5s = {
        let s = state.read().await;
        s.recent_recommendations.clone()
    };

    let active_mods = if filter_nomod { filter_mods & !(Mods::DOUBLETIME | Mods::HARDROCK).bits() } else { filter_mods };

    let mut best_match: Option<(Beatmap, String, NpBreakdown)> = None;
    let mut best_diff = f64::MAX;

    let filtered_candidates: Vec<Beatmap> = {
        let not_recent: Vec<Beatmap> = candidate_maps.iter().filter(|m| !recent_md5s.contains(&m.file_md5)).cloned().collect();
        if not_recent.is_empty() {
            candidate_maps.clone()
        } else {
            not_recent
        }
    };

    for bmap in filtered_candidates.iter().take(30) {
        let content = {
            let s = state.read().await;
            utils::get_or_fetch_beatmap_content(&s.http, &s.db, &s.config, bmap).await
        };

        if let Some(content) = content {
            if let Some(breakdown) = calculate_np_breakdown(bmap, &content, active_mods) {
                let diff = (breakdown.pp100 - target_pp).abs();
                if diff < best_diff {
                    best_diff = diff;
                    best_match = Some((bmap.clone(), content, breakdown));
                    if diff < 15.0 {
                        break;
                    }
                }
            }
        }
    }

    let Some((selected_map, _content, breakdown)) = best_match else {
        reply(state, target, "Could not find a suitable recommendation at your skill level. Try adding more songs!").await;
        return;
    };

    let link = if selected_map.beatmap_id > 0 {
        format!("https://osu.ppy.sh/b/{}", selected_map.beatmap_id)
    } else {
        format!("https://osu.ppy.sh/s/{}", selected_map.beatmapset_id)
    };

    let mods_part = if breakdown.mods_str.is_empty() {
        String::new()
    } else {
        format!(" {}", breakdown.mods_str)
    };

    let rep = format!(
        "[{} {} - {} [{}]{}] ★ {:.2} | 95%: {:.0}pp | 98%: {:.0}pp | 99%: {:.0}pp | 100%: {:.0}pp",
        link,
        selected_map.artist,
        selected_map.title,
        selected_map.version,
        mods_part,
        breakdown.stars,
        breakdown.pp95,
        breakdown.pp98,
        breakdown.pp99,
        breakdown.pp100,
    );

    {
        let mut s = state.write().await;
        s.last_np_map = Some(selected_map.clone());
        if !s.recent_recommendations.contains(&selected_map.file_md5) {
            s.recent_recommendations.push(selected_map.file_md5.clone());
            if s.recent_recommendations.len() > 20 {
                s.recent_recommendations.remove(0);
            }
        }
    }

    reply(state, target, &rep).await;
}

pub async fn handle_help(state: &Arc<RwLock<AppState>>, target: &str) {
    let msg = "Tillerino Commands:\n\
        /np : PP calculations for current song (95%, 98%, 99%, 100% FC)\n\
        !r [nomod/dt/hr/hd] : Recommend a beatmap tailored to your skill\n\
        !with <mods> : Calculate PP with custom mods (e.g. !with HDDT, !with HR)\n\
        !acc <percentage> : Calculate PP for a specific accuracy (e.g. !acc 97.5)";

    reply(state, target, msg).await;
}
