use crate::db;
use crate::multiplayer::bot::{sanitize_freemod_bits, BotSource, SkillProfile};
use crate::multiplayer::is_synthetic_bot_id;
use crate::multiplayer::match_state::{MatchRoom, MatchSlot, SlotStatus, Team, TeamType, WinCondition};
use crate::multiplayer::simulator;
use crate::packets;
use crate::server::response::Response;
use crate::state::AppState;
use crate::types::mods::Mods;
use crate::types::player::Player;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const BANCHOBOT_ID: i32 = 3;
pub const TILLERINO_ID: i32 = 4;

/// Safely emits `ChoUserLogout` for removed multiplayer bots without logging out
/// real friends, BanchoBot, Tillerino, or the local player.
pub fn safe_logout_bots(player: &mut Player, removed_bot_ids: &[i32], friend_ids: &[i32]) {
    for &uid in removed_bot_ids {
        if uid == player.userid || uid == BANCHOBOT_ID || uid == TILLERINO_ID || uid <= 2 {
            continue;
        }
        if is_synthetic_bot_id(uid) || !friend_ids.contains(&uid) {
            player.queue.extend_from_slice(&packets::logout(uid));
        }
    }
}

/// Asynchronously locates and caches a beatmap's `.osu` file content in SQLite so
/// `OsuMatchStart` never blocks the `/cho` poll loop with a synchronous `Songs/` scan.
fn spawn_prefetch_beatmap_osu(state: Arc<RwLock<AppState>>, md5: String, beatmap_id: i32, beatmap_name: String) {
    if md5.is_empty() {
        return;
    }
    tokio::spawn(async move {
        let songs_dir = {
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            if let Ok(Some(b)) = db::get_beatmap_by_md5(&db_conn, &md5) {
                if b.file_content.is_some() {
                    return;
                }
            }
            drop(db_conn);
            utils::resolve_songs_folder(&s.config)
        };
        let Some(sdir) = songs_dir else {
            return;
        };

        let md5_clone = md5.clone();
        let id_opt = if beatmap_id > 0 { Some(beatmap_id as i64) } else { None };
        let hint_opt = if beatmap_name.is_empty() { None } else { Some(beatmap_name) };

        let found = tokio::task::spawn_blocking(move || {
            utils::find_and_parse_local_osu_file(&sdir, None, id_opt, Some(md5_clone.as_str()), hint_opt.as_deref())
        })
        .await
        .ok()
        .flatten();

        if let Some((local_bmap, content)) = found {
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            let _ = db::insert_beatmap(&db_conn, &local_bmap);
            let _ = db::update_beatmap_file_content(&db_conn, &local_bmap.file_md5, &content);
        }
    });
}

fn frame_accuracy_pct(f: &packets::ScoreFrame, mode: u8) -> f64 {
    match mode {
        3 => {
            let total = f.ngeki as f64 + f.n300 as f64 + f.nkatu as f64 + f.n100 as f64 + f.n50 as f64 + f.nmiss as f64;
            if total <= 0.0 {
                return 100.0;
            }
            let weighted = (f.ngeki as f64 + f.n300 as f64) * 300.0 + f.nkatu as f64 * 200.0 + f.n100 as f64 * 100.0 + f.n50 as f64 * 50.0;
            (weighted / (total * 300.0)) * 100.0
        }
        1 => {
            let total = f.n300 as f64 + f.n100 as f64 + f.nmiss as f64;
            if total <= 0.0 {
                return 100.0;
            }
            ((f.n300 as f64 + f.n100 as f64 * 0.5) / total) * 100.0
        }
        _ => {
            let total = f.n300 as f64 + f.n100 as f64 + f.n50 as f64 + f.nmiss as f64;
            if total <= 0.0 {
                return 100.0;
            }
            ((f.n300 as f64 * 300.0 + f.n100 as f64 * 100.0 + f.n50 as f64 * 50.0) / (total * 300.0)) * 100.0
        }
    }
}

fn finalize_match(room: &mut MatchRoom, player: &mut Player) {
    struct Entry {
        name: String,
        team: Team,
        score: i32,
        acc: f64,
        max_combo: u16,
        failed: bool,
    }

    let mut entries: Vec<Entry> = Vec::new();
    let mode = room.mode;

    // Collect human entry
    if let Some(human_idx) = room.find_human_slot(player.userid) {
        let team = room.slots[human_idx].team;
        let (score, acc, max_combo) = if let Some(ref f) = room.human_last_frame {
            (f.total_score, frame_accuracy_pct(f, mode), f.max_combo)
        } else {
            (0, 0.0, 0)
        };
        entries.push(Entry {
            name: player.name.clone(),
            team,
            score,
            acc,
            max_combo,
            failed: room.human_failed,
        });
    }

    // Flush final frames for all bots and collect their results
    for (slot_idx, slot) in room.slots.iter_mut().enumerate() {
        let team = slot.team;
        if let Some(ref mut bot) = slot.bot {
            if !bot.has_failed && bot.fail_time_ms.is_some() {
                bot.has_failed = true;
                player.queue.extend_from_slice(&packets::match_player_failed(slot_idx as i32));
            }
            if let Some(last_frame) = bot.timeline.last() {
                if bot.last_sent_idx < bot.timeline.len() {
                    let mut final_frame = last_frame.clone();
                    final_frame.id = slot_idx as u8;
                    player.queue.extend_from_slice(&packets::match_score_update(&final_frame));
                    bot.last_sent_idx = bot.timeline.len();
                }
                entries.push(Entry {
                    name: bot.profile.name.clone(),
                    team,
                    score: last_frame.total_score,
                    acc: frame_accuracy_pct(last_frame, mode),
                    max_combo: last_frame.max_combo,
                    failed: bot.has_failed,
                });
            }
        }
    }

    let summary = if room.team_type.is_team_mode() {
        let red_total: i64 = entries.iter().filter(|e| e.team == Team::Red && !e.failed).map(|e| e.score as i64).sum();
        let blue_total: i64 = entries.iter().filter(|e| e.team == Team::Blue && !e.failed).map(|e| e.score as i64).sum();
        let winner = if red_total > blue_total {
            "Red Team wins!"
        } else if blue_total > red_total {
            "Blue Team wins!"
        } else {
            "It's a tie!"
        };
        format!("Match finished — {} (Red: {} vs Blue: {})", winner, red_total, blue_total)
    } else {
        entries.sort_by(|a, b| {
            a.failed.cmp(&b.failed).then_with(|| match room.win_condition {
                WinCondition::Accuracy => b.acc.partial_cmp(&a.acc).unwrap_or(std::cmp::Ordering::Equal),
                WinCondition::Combo => b.max_combo.cmp(&a.max_combo),
                WinCondition::Score | WinCondition::ScoreV2 => b.score.cmp(&a.score),
            })
        });
        let board: Vec<String> = entries
            .iter()
            .take(4)
            .enumerate()
            .map(|(i, e)| {
                let fail_tag = if e.failed { " [FAIL]" } else { "" };
                format!("#{} {} ({}, {:.1}%, {}x){}", i + 1, e.name, e.score, e.acc, e.max_combo, fail_tag)
            })
            .collect();
        if board.is_empty() {
            "Match finished!".to_string()
        } else {
            format!("Match finished — {}", board.join(" | "))
        }
    };

    room.reset_after_match();
    let wire = room.to_wire();
    player.queue.extend_from_slice(&packets::match_complete());
    player.queue.extend_from_slice(&packets::update_match(&wire, true));
    player.queue.extend_from_slice(&packets::send_msg("BanchoBot", &summary, "#multiplayer", BANCHOBOT_ID));
}

pub fn create_bot_player(name: &str, userid: i32) -> Player {
    let mut bot = Player::new(name.to_string());
    bot.userid = userid;
    bot.bancho_privs = 1;
    bot.rank = 0;
    bot.pp = 0;
    bot.acc = 0.0;
    bot.country = 0;
    bot.ranked_score = 0;
    bot.total_score = 0;
    bot.playcount = 0;
    bot.action = 0;
    bot.info_text = String::new();
    bot
}

const CHANNELS: &[(&str, &str)] = &[("#osu", "x"), ("#recent", "shows recently submitted scores!"), ("#tops", "shows top plays, as well as updates them!")];

pub async fn bancho_connect(state: Arc<RwLock<AppState>>, params: &std::collections::HashMap<String, String>) -> Response {
    let username = params.get("u").map(|u| utils::url_decode(u)).unwrap_or_default().trim().to_string();

    if !username.is_empty() {
        let mut s = state.write().await;
        s.pending_login_name = Some(username.clone());
        utils::log(&format!("Got a player name of {}", username));
    }

    Response::empty()
}

pub async fn handle(state: Arc<RwLock<AppState>>, osu_token: Option<&str>, body_bytes: &[u8]) -> Response {
    match osu_token {
        None | Some("") => {
            let (body, token) = login(state, body_bytes).await;
            Response::new(body).with_header("cho-token", &token)
        }
        Some(_) => {
            let player_name = {
                let s = state.read().await;
                if s.player.is_none() {
                    return Response::new(packets::system_restart(0));
                }
                s.player.as_ref().unwrap().name.clone()
            };

            let in_packets = packets::split_packets(body_bytes);
            for in_pkt in in_packets {
                if crate::logger::is_debug() {
                    crate::logger::debug(&format!("Bancho packet: ID {} ({} bytes)", in_pkt.id, in_pkt.payload.len()));
                }
                match in_pkt.id {
                    x if x == packets::PacketId::OsuPing as u16 => {}
                    x if x == packets::PacketId::OsuRequestStatusUpdate as u16 => {
                        let mut s = state.write().await;
                        if let Some(ref mut p) = s.player {
                            p.enqueue_stats();
                        }
                    }
                    x if x == packets::PacketId::OsuChangeAction as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let (Ok(action), Ok(info_text), Ok(map_md5), Ok(mods), Ok(mode), Ok(map_id)) =
                            (reader.read_u8(), reader.read_string(), reader.read_string(), reader.read_u32(), reader.read_u8(), reader.read_i32())
                        {
                            let mut resolved_bmap = None;
                            {
                                let s = state.read().await;
                                let db_conn = s.db.lock().await;
                                if !map_md5.is_empty() {
                                    resolved_bmap = db::get_beatmap_by_md5(&db_conn, &map_md5).ok().flatten();
                                }
                                if resolved_bmap.is_none() && map_id > 0 {
                                    if let Ok(Some(b)) = db::get_beatmap_by_id(&db_conn, map_id as i64) {
                                        if map_md5.is_empty() || b.file_md5.eq_ignore_ascii_case(&map_md5) {
                                            resolved_bmap = Some(b);
                                        }
                                    }
                                }
                            }

                            let mut s = state.write().await;
                            if let Some(b) = resolved_bmap {
                                s.last_np_map = Some(b);
                            } else if !map_md5.is_empty() && s.last_np_map.as_ref().map(|b| &b.file_md5) != Some(&map_md5) {
                                s.last_np_map = None;
                            }
                            if let Some(ref mut p) = s.player {
                                p.action = action as i8;
                                p.info_text = info_text;
                                p.map_md5 = map_md5;
                                p.mods = mods;
                                p.mode = mode;
                                p.map_id = map_id;
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuSendPublicMessage as u16 || x == packets::PacketId::OsuSendPrivateMessage as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let (Ok(_sender), Ok(message), Ok(target), Ok(_userid)) =
                            (reader.read_string(), reader.read_string(), reader.read_string(), reader.read_i32())
                        {
                            crate::handlers::chat_commands::handle_chat_message(state.clone(), &player_name, &message, &target).await;
                        }
                    }
                    x if x == packets::PacketId::OsuUserPresenceRequestAll as u16 => {
                        let friend_records = {
                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            db::get_friend_records(&db_conn, &player_name).unwrap_or_default()
                        };

                        let mut s = state.write().await;
                        if let Some(ref mut p) = s.player {
                            let player_id = p.userid;
                            let mut all_ids = Vec::with_capacity(3 + friend_records.len());
                            all_ids.push(player_id);
                            all_ids.push(BANCHOBOT_ID);
                            all_ids.push(TILLERINO_ID);
                            for f in &friend_records {
                                if f.friend_id > 2 && f.friend_id != player_id && f.friend_id != 2070907 && !all_ids.contains(&f.friend_id) {
                                    all_ids.push(f.friend_id);
                                }
                            }
                            p.queue.extend_from_slice(&packets::user_presence_bundle(&all_ids));

                            p.queue.extend_from_slice(&packets::user_presence(p));
                            p.enqueue_stats();

                            let bancho = create_bot_player("BanchoBot", BANCHOBOT_ID);
                            let tillerino = create_bot_player("Tillerino", TILLERINO_ID);
                            p.queue.extend_from_slice(&packets::user_presence(&bancho));
                            p.queue.extend_from_slice(&packets::user_stats(&bancho));
                            p.queue.extend_from_slice(&packets::user_presence(&tillerino));
                            p.queue.extend_from_slice(&packets::user_stats(&tillerino));

                            for f in friend_records {
                                if f.friend_id <= 2
                                    || f.friend_id == player_id
                                    || f.friend_id == BANCHOBOT_ID
                                    || f.friend_id == TILLERINO_ID
                                    || f.friend_id == 2070907
                                {
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
                    x if x == packets::PacketId::OsuUserPresenceRequest as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(requested_ids) = reader.read_i32_list() {
                            let friend_records = {
                                let s = state.read().await;
                                let db_conn = s.db.lock().await;
                                db::get_friend_records(&db_conn, &player_name).unwrap_or_default()
                            };

                            let mut s = state.write().await;
                            let bot_players: Vec<Player> = s
                                .active_match
                                .as_ref()
                                .map(|m| {
                                    m.slots
                                        .iter()
                                        .filter_map(|slot| slot.bot.as_ref().map(|b| b.profile.to_bancho_player(m.mode)))
                                        .collect()
                                })
                                .unwrap_or_default();
                            if let Some(ref mut p) = s.player {
                                for target_id in requested_ids {
                                    if target_id == p.userid {
                                        p.queue.extend_from_slice(&packets::user_presence(p));
                                    } else if target_id == BANCHOBOT_ID {
                                        let bot = create_bot_player("BanchoBot", BANCHOBOT_ID);
                                        p.queue.extend_from_slice(&packets::user_presence(&bot));
                                    } else if target_id == TILLERINO_ID || target_id == 2070907 {
                                        let bot = create_bot_player("Tillerino", TILLERINO_ID);
                                        p.queue.extend_from_slice(&packets::user_presence(&bot));
                                    } else if let Some(bp) = bot_players.iter().find(|b| b.userid == target_id) {
                                        p.queue.extend_from_slice(&packets::user_presence(bp));
                                    } else if target_id > 2 {
                                        if let Some(f) = friend_records.iter().find(|fr| fr.friend_id == target_id) {
                                            let display_name = if f.friend_name.is_empty() { format!("Friend {}", f.friend_id) } else { f.friend_name.clone() };
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
                                        }
                                    }
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuUserStatsRequest as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(requested_ids) = reader.read_i32_list() {
                            let friend_records = {
                                let s = state.read().await;
                                let db_conn = s.db.lock().await;
                                db::get_friend_records(&db_conn, &player_name).unwrap_or_default()
                            };

                            let mut s = state.write().await;
                            let bot_players: Vec<Player> = s
                                .active_match
                                .as_ref()
                                .map(|m| {
                                    m.slots
                                        .iter()
                                        .filter_map(|slot| slot.bot.as_ref().map(|b| b.profile.to_bancho_player(m.mode)))
                                        .collect()
                                })
                                .unwrap_or_default();
                            if let Some(ref mut p) = s.player {
                                for target_id in requested_ids {
                                    if target_id == p.userid {
                                        p.enqueue_stats();
                                    } else if target_id == BANCHOBOT_ID {
                                        let bot = create_bot_player("BanchoBot", BANCHOBOT_ID);
                                        p.queue.extend_from_slice(&packets::user_stats(&bot));
                                    } else if target_id == TILLERINO_ID || target_id == 2070907 {
                                        let bot = create_bot_player("Tillerino", TILLERINO_ID);
                                        p.queue.extend_from_slice(&packets::user_stats(&bot));
                                    } else if let Some(bp) = bot_players.iter().find(|b| b.userid == target_id) {
                                        p.queue.extend_from_slice(&packets::user_stats(bp));
                                    } else if target_id > 2 {
                                        if let Some(f) = friend_records.iter().find(|fr| fr.friend_id == target_id) {
                                            let display_name = if f.friend_name.is_empty() { format!("Friend {}", f.friend_id) } else { f.friend_name.clone() };
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
                                            p.queue.extend_from_slice(&packets::user_stats(&fp));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuFriendAdd as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(friend_id) = reader.read_i32() {
                            let player_id = {
                                let s = state.read().await;
                                s.player.as_ref().map(|p| p.userid).unwrap_or(2)
                            };
                            if friend_id <= 2 || friend_id == player_id {
                                continue;
                            }
                            let (api_key, http) = {
                                let s = state.read().await;
                                (s.config.osu_api_key.clone(), s.http.clone())
                            };

                            let f_rec = if friend_id == TILLERINO_ID || friend_id == 2070907 {
                                db::FriendRecord {
                                    friend_id: TILLERINO_ID,
                                    friend_name: "Tillerino".to_string(),
                                    rank: 1,
                                    pp: 0,
                                    acc: 0.0,
                                    country: 0,
                                    ranked_score: 0,
                                    total_score: 0,
                                    playcount: 0,
                                }
                            } else if friend_id == BANCHOBOT_ID {
                                db::FriendRecord {
                                    friend_id: BANCHOBOT_ID,
                                    friend_name: "BanchoBot".to_string(),
                                    rank: 1,
                                    pp: 0,
                                    acc: 0.0,
                                    country: 0,
                                    ranked_score: 0,
                                    total_score: 0,
                                    playcount: 0,
                                }
                            } else {
                                let mut friend_rec =
                                    if let Some(ref key) = api_key { utils::fetch_user_stats_from_api(&http, key, &friend_id.to_string()).await } else { None };
                                friend_rec.take().unwrap_or_else(|| db::FriendRecord {
                                    friend_id,
                                    friend_name: format!("Friend {}", friend_id),
                                    rank: 1,
                                    pp: 0,
                                    acc: 0.0,
                                    country: 0,
                                    ranked_score: 0,
                                    total_score: 0,
                                    playcount: 0,
                                })
                            };

                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            let _ = db::save_friend_stats(&db_conn, &player_name, &f_rec);
                            let mut friend_ids = db::get_friend_ids(&db_conn, &player_name).unwrap_or_default();
                            if !friend_ids.contains(&BANCHOBOT_ID) {
                                friend_ids.push(BANCHOBOT_ID);
                            }
                            if !friend_ids.contains(&TILLERINO_ID) {
                                friend_ids.push(TILLERINO_ID);
                            }
                            drop(db_conn);
                            drop(s);

                            let mut s = state.write().await;
                            if let Some(ref mut p) = s.player {
                                p.queue.extend_from_slice(&packets::friends_list(&friend_ids));

                                if f_rec.friend_id != BANCHOBOT_ID && f_rec.friend_id != TILLERINO_ID {
                                    let mut fp = Player::new(f_rec.friend_name.clone());
                                    fp.userid = f_rec.friend_id;
                                    fp.bancho_privs = 1;
                                    fp.rank = f_rec.rank;
                                    fp.pp = f_rec.pp;
                                    fp.acc = f_rec.acc;
                                    fp.country = f_rec.country;
                                    fp.ranked_score = f_rec.ranked_score;
                                    fp.total_score = f_rec.total_score;
                                    fp.playcount = f_rec.playcount;
                                    fp.action = 0;
                                    fp.info_text = String::new();
                                    p.queue.extend_from_slice(&packets::user_presence(&fp));
                                    p.queue.extend_from_slice(&packets::user_stats(&fp));
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuFriendRemove as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(friend_id) = reader.read_i32() {
                            if friend_id <= 2 {
                                continue;
                            }
                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            let _ = db::remove_friend(&db_conn, &player_name, friend_id);
                            if friend_id == TILLERINO_ID || friend_id == 2070907 {
                                let _ = db::remove_friend(&db_conn, &player_name, 2070907);
                                let _ = db::remove_friend(&db_conn, &player_name, TILLERINO_ID);
                            }
                            let mut friend_ids = db::get_friend_ids(&db_conn, &player_name).unwrap_or_default();
                            friend_ids.retain(|&id| id > 2);
                            if !friend_ids.contains(&BANCHOBOT_ID) {
                                friend_ids.push(BANCHOBOT_ID);
                            }
                            if !friend_ids.contains(&TILLERINO_ID) {
                                friend_ids.push(TILLERINO_ID);
                            }
                            drop(db_conn);
                            drop(s);

                            let mut s = state.write().await;
                            if let Some(ref mut p) = s.player {
                                p.queue.extend_from_slice(&packets::friends_list(&friend_ids));
                                if friend_id != BANCHOBOT_ID && friend_id != TILLERINO_ID {
                                    p.queue.extend_from_slice(&packets::logout(friend_id));
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuChannelJoin as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(channel) = reader.read_string() {
                            let mut s = state.write().await;
                            if let Some(ref mut p) = s.player {
                                p.queue.extend_from_slice(&packets::channel_join(&channel));
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuJoinLobby as u16 => {
                        let mut s = state.write().await;
                        s.in_lobby = true;
                        let wire_opt = s.active_match.as_ref().map(|m| m.to_wire());
                        if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_opt) {
                            p.queue.extend_from_slice(&packets::new_match(&wire, false));
                        }
                    }
                    x if x == packets::PacketId::OsuPartLobby as u16 => {
                        let mut s = state.write().await;
                        s.in_lobby = false;
                    }
                    x if x == packets::PacketId::OsuCreateMatch as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(wire_in) = reader.read_match() {
                            let friend_ids = {
                                let s = state.read().await;
                                let db_conn = s.db.lock().await;
                                db::get_friend_ids(&db_conn, &player_name).unwrap_or_default()
                            };

                            let mut s = state.write().await;
                            let prev_bot_ids: Vec<i32> = s
                                .active_match
                                .take()
                                .map(|mut old| old.clear_bots().into_iter().map(|b| b.userid).collect())
                                .unwrap_or_default();

                            let mut prefetch_osu: Option<(String, i32, String)> = None;
                            if let Some(ref mut p) = s.player {
                                if !prev_bot_ids.is_empty() {
                                    safe_logout_bots(p, &prev_bot_ids, &friend_ids);
                                }
                                let room = MatchRoom::from_create_wire(1, p.userid, &wire_in);
                                let wire_out = room.to_wire();
                                if !room.beatmap_md5.is_empty() {
                                    prefetch_osu = Some((room.beatmap_md5.clone(), room.beatmap_id, room.beatmap_name.clone()));
                                }
                                p.queue.extend_from_slice(&packets::match_join_success(&wire_out));
                                p.queue.extend_from_slice(&packets::channel_join("#multiplayer"));
                                p.queue.extend_from_slice(&packets::send_msg(
                                    "BanchoBot",
                                    "Simulated MP room ready! Type !mp addbot <5digit|rival|username> to add bots, or !mp help for commands.",
                                    "#multiplayer",
                                    BANCHOBOT_ID,
                                ));
                                s.active_match = Some(room);
                            }
                            drop(s);
                            if let Some((md5, bmap_id, bmap_name)) = prefetch_osu {
                                spawn_prefetch_beatmap_osu(state.clone(), md5, bmap_id, bmap_name);
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuJoinMatch as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let (Ok(match_id), Ok(password)) = (reader.read_i32(), reader.read_string()) {
                            let mut s = state.write().await;
                            let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                            let mut joined_wire = None;
                            let mut bot_players = Vec::new();

                            if let Some(ref mut room) = s.active_match {
                                if room.id as i32 == match_id && (room.password.is_empty() || room.password == password) {
                                    if room.find_human_slot(player_id).is_none() {
                                        if let Some(open_idx) = room.find_open_slot() {
                                            room.slots[open_idx] = MatchSlot {
                                                status: SlotStatus::NotReady,
                                                team: if room.team_type.is_team_mode() { Team::Blue } else { Team::Neutral },
                                                user_id: player_id,
                                                mods: 0,
                                                bot: None,
                                            };
                                        }
                                    }
                                    for slot in &room.slots {
                                        if let Some(ref b) = slot.bot {
                                            bot_players.push(b.profile.to_bancho_player(room.mode));
                                        }
                                    }
                                    joined_wire = Some(room.to_wire());
                                }
                            }

                            if let Some(ref mut p) = s.player {
                                if let Some(wire) = joined_wire {
                                    for bp in &bot_players {
                                        p.queue.extend_from_slice(&packets::user_presence(bp));
                                        p.queue.extend_from_slice(&packets::user_stats(bp));
                                    }
                                    p.queue.extend_from_slice(&packets::match_join_success(&wire));
                                    p.queue.extend_from_slice(&packets::channel_join("#multiplayer"));
                                } else {
                                    p.queue.extend_from_slice(&packets::match_join_fail());
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuPartMatch as u16 => {
                        let friend_ids = {
                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            db::get_friend_ids(&db_conn, &player_name).unwrap_or_default()
                        };

                        let mut s = state.write().await;
                        let in_lobby = s.in_lobby;
                        let parted = s.active_match.take();
                        if let Some(ref mut p) = s.player {
                            p.queue.extend_from_slice(&packets::channel_kick("#multiplayer"));
                            if let Some(mut room) = parted {
                                let room_id = room.id as i32;
                                let removed_ids: Vec<i32> = room.clear_bots().into_iter().map(|b| b.userid).collect();
                                safe_logout_bots(p, &removed_ids, &friend_ids);
                                if in_lobby {
                                    p.queue.extend_from_slice(&packets::dispose_match(room_id));
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchChangeSettings as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(wire_in) = reader.read_match() {
                            let mut prefetch_target: Option<(i64, u8)> = None;
                            let mut prefetch_osu: Option<(String, i32, String)> = None;
                            {
                                let mut s = state.write().await;
                                let wire_out = if let Some(ref mut room) = s.active_match {
                                    let map_changed = room.beatmap_md5 != wire_in.beatmap_md5 || room.beatmap_id != wire_in.beatmap_id;
                                    if !wire_in.name.is_empty() {
                                        room.name = wire_in.name;
                                    }
                                    room.beatmap_name = wire_in.beatmap_name;
                                    room.beatmap_id = wire_in.beatmap_id;
                                    room.beatmap_md5 = wire_in.beatmap_md5;
                                    room.mods = wire_in.mods;
                                    room.mode = wire_in.mode;
                                    room.win_condition = WinCondition::from_u8(wire_in.win_condition);
                                    room.team_type = TeamType::from_u8(wire_in.team_type);
                                    room.freemods = wire_in.freemods != 0;

                                    if map_changed {
                                        for slot in &mut room.slots {
                                            if slot.is_human() && slot.status == SlotStatus::Ready {
                                                slot.status = SlotStatus::NotReady;
                                            }
                                        }
                                        if room.beatmap_id > 0 {
                                            prefetch_target = Some((room.beatmap_id as i64, room.mode));
                                        }
                                        if !room.beatmap_md5.is_empty() {
                                            prefetch_osu = Some((room.beatmap_md5.clone(), room.beatmap_id, room.beatmap_name.clone()));
                                        }
                                    }
                                    room.set_all_bots_ready();
                                    room.apply_freemod_constraints();
                                    room.assign_balanced_teams();
                                    Some(room.to_wire())
                                } else {
                                    None
                                };
                                if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                                    p.queue.extend_from_slice(&packets::update_match(&wire, true));
                                }
                            }
                            if let Some((bmap_id, mode)) = prefetch_target {
                                crate::multiplayer::mimic::spawn_prefetch_mimic_scores(state.clone(), bmap_id, mode);
                            }
                            if let Some((md5, bmap_id, bmap_name)) = prefetch_osu {
                                spawn_prefetch_beatmap_osu(state.clone(), md5, bmap_id, bmap_name);
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchLock as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(slot_idx_i32) = reader.read_i32() {
                            if (0..16).contains(&slot_idx_i32) {
                                let idx = slot_idx_i32 as usize;
                                let friend_ids = {
                                    let s = state.read().await;
                                    let db_conn = s.db.lock().await;
                                    db::get_friend_ids(&db_conn, &player_name).unwrap_or_default()
                                };

                                let mut s = state.write().await;
                                let mut kicked_bot_id = None;
                                let wire_out = if let Some(ref mut room) = s.active_match {
                                    if room.slots[idx].is_bot() {
                                        if let Some(removed) = room.remove_bot_at(idx) {
                                            kicked_bot_id = Some(removed.userid);
                                        }
                                        room.slots[idx].status = SlotStatus::Locked;
                                    } else if room.slots[idx].status == SlotStatus::Open {
                                        room.slots[idx].status = SlotStatus::Locked;
                                    } else if room.slots[idx].status == SlotStatus::Locked {
                                        room.slots[idx].status = SlotStatus::Open;
                                    }
                                    Some(room.to_wire())
                                } else {
                                    None
                                };
                                if let Some(ref mut p) = s.player {
                                    if let Some(uid) = kicked_bot_id {
                                        safe_logout_bots(p, &[uid], &friend_ids);
                                    }
                                    if let Some(wire) = wire_out {
                                        p.queue.extend_from_slice(&packets::update_match(&wire, true));
                                    }
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchTransferHost as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(slot_idx_i32) = reader.read_i32() {
                            if (0..16).contains(&slot_idx_i32) {
                                let idx = slot_idx_i32 as usize;
                                let mut s = state.write().await;
                                let mut blocked_bot = false;
                                let wire_out = if let Some(ref mut room) = s.active_match {
                                    if room.slots[idx].is_bot() {
                                        blocked_bot = true;
                                        None
                                    } else if room.slots[idx].is_human() {
                                        room.host_id = room.slots[idx].user_id;
                                        Some(room.to_wire())
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };
                                if let Some(ref mut p) = s.player {
                                    if blocked_bot {
                                        p.queue.extend_from_slice(&packets::send_msg(
                                            "BanchoBot",
                                            "Bots cannot hold host.",
                                            "#multiplayer",
                                            BANCHOBOT_ID,
                                        ));
                                    } else if let Some(wire) = wire_out {
                                        p.queue.extend_from_slice(&packets::update_match(&wire, true));
                                    }
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchChangeSlot as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(target_idx_i32) = reader.read_i32() {
                            if (0..16).contains(&target_idx_i32) {
                                let target_idx = target_idx_i32 as usize;
                                let mut s = state.write().await;
                                let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                                let wire_out = if let Some(ref mut room) = s.active_match {
                                    if room.slots[target_idx].is_open() {
                                        if let Some(cur_idx) = room.find_human_slot(player_id) {
                                            let moved = room.slots[cur_idx].clone();
                                            room.slots[cur_idx] = MatchSlot::open();
                                            room.slots[target_idx] = moved;
                                            Some(room.to_wire())
                                        } else {
                                            None
                                        }
                                    } else {
                                        None
                                    }
                                } else {
                                    None
                                };
                                if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                                    p.queue.extend_from_slice(&packets::update_match(&wire, true));
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchReady as u16 => {
                        let mut s = state.write().await;
                        let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                        let wire_out = s.active_match.as_mut().and_then(|room| {
                            let idx = room.find_human_slot(player_id)?;
                            room.slots[idx].status = SlotStatus::Ready;
                            Some(room.to_wire())
                        });
                        if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                            p.queue.extend_from_slice(&packets::update_match(&wire, true));
                        }
                    }
                    x if x == packets::PacketId::OsuMatchNotReady as u16 => {
                        let mut s = state.write().await;
                        let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                        let wire_out = s.active_match.as_mut().and_then(|room| {
                            let idx = room.find_human_slot(player_id)?;
                            room.slots[idx].status = SlotStatus::NotReady;
                            Some(room.to_wire())
                        });
                        if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                            p.queue.extend_from_slice(&packets::update_match(&wire, true));
                        }
                    }
                    x if x == packets::PacketId::OsuMatchNoBeatmap as u16 => {
                        let mut s = state.write().await;
                        let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                        let wire_out = s.active_match.as_mut().and_then(|room| {
                            let idx = room.find_human_slot(player_id)?;
                            room.slots[idx].status = SlotStatus::NoMap;
                            Some(room.to_wire())
                        });
                        if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                            p.queue.extend_from_slice(&packets::update_match(&wire, true));
                        }
                    }
                    x if x == packets::PacketId::OsuMatchHasBeatmap as u16 => {
                        let mut s = state.write().await;
                        let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                        let wire_out = s.active_match.as_mut().and_then(|room| {
                            let idx = room.find_human_slot(player_id)?;
                            room.slots[idx].status = SlotStatus::NotReady;
                            Some(room.to_wire())
                        });
                        if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                            p.queue.extend_from_slice(&packets::update_match(&wire, true));
                        }
                    }
                    x if x == packets::PacketId::OsuMatchChangeMods as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(mods) = reader.read_u32() {
                            let mut s = state.write().await;
                            let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                            let wire_out = s.active_match.as_mut().map(|room| {
                                if room.freemods {
                                    room.mods = mods & Mods::SPEED_CHANGING.bits();
                                    if let Some(idx) = room.find_human_slot(player_id) {
                                        room.slots[idx].mods = sanitize_freemod_bits(mods);
                                    }
                                } else {
                                    room.mods = mods;
                                }
                                room.to_wire()
                            });
                            if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                                p.queue.extend_from_slice(&packets::update_match(&wire, true));
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchChangeTeam as u16 => {
                        let mut s = state.write().await;
                        let player_id = s.player.as_ref().map(|p| p.userid).unwrap_or(2);
                        let wire_out = s.active_match.as_mut().and_then(|room| {
                            if !room.team_type.is_team_mode() {
                                return None;
                            }
                            let idx = room.find_human_slot(player_id)?;
                            room.slots[idx].team = match room.slots[idx].team {
                                Team::Blue => Team::Red,
                                _ => Team::Blue,
                            };
                            Some(room.to_wire())
                        });
                        if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                            p.queue.extend_from_slice(&packets::update_match(&wire, true));
                        }
                    }
                    x if x == packets::PacketId::OsuMatchChangePassword as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(wire_in) = reader.read_match() {
                            let mut s = state.write().await;
                            let wire_out = s.active_match.as_mut().map(|room| {
                                room.password = wire_in.password;
                                room.to_wire()
                            });
                            if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                                p.queue.extend_from_slice(&packets::update_match(&wire, true));
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchStart as u16 => {
                        // Snapshot room & bot metadata outside write lock
                        let start_snapshot = {
                            let s = state.read().await;
                            let now = std::time::Instant::now();
                            s.active_match.as_ref().and_then(|room| {
                                if room.in_progress {
                                    return None;
                                }
                                let bmap_id = room.beatmap_id as i64;
                                let bots: Vec<(usize, i32, SkillProfile, u32, Option<simulator::ConstrainedScore>)> = room
                                    .slots
                                    .iter()
                                    .enumerate()
                                    .filter_map(|(idx, slot)| {
                                        let bot = slot.bot.as_ref()?;
                                        let (skill, cached_score) = match &bot.profile.source {
                                            BotSource::Procedural(sk) => (sk.clone(), None),
                                            BotSource::MimicPlayer { osu_user_id, fallback_skill, .. } => {
                                                let sc = osu_user_id.and_then(|uid| {
                                                    if bmap_id <= 0 {
                                                        return None;
                                                    }
                                                    s.mp_map_score_cache
                                                        .get(&(uid, bmap_id))
                                                        .filter(|(ts, _)| {
                                                            crate::multiplayer::mimic::is_cache_fresh(
                                                                *ts,
                                                                crate::multiplayer::mimic::MAP_SCORE_CACHE_TTL,
                                                                now,
                                                            )
                                                        })
                                                        .and_then(|(_, opt)| opt.clone())
                                                });
                                                (fallback_skill.clone(), sc)
                                            }
                                            BotSource::PersonalBest | BotSource::ReplayGhost { .. } => (SkillProfile::default(), None),
                                        };
                                        Some((idx, bot.profile.userid, skill, room.effective_mods_for_slot(idx), cached_score))
                                    })
                                    .collect();
                                Some((
                                    room.beatmap_md5.clone(),
                                    room.mode,
                                    room.win_condition,
                                    room.rng_seed_override,
                                    room.seed,
                                    room.run_nonce,
                                    bots,
                                ))
                            })
                        };

                        if let Some((md5, mode, win_condition, seed_override, wire_seed, run_nonce, bots)) = start_snapshot {
                            // Fast DB-only .osu lookup (never block /cho with a full Songs/ disk scan)
                            let osu_content: Option<String> = if !md5.is_empty() {
                                let s = state.read().await;
                                let db_conn = s.db.lock().await;
                                db::get_beatmap_by_md5(&db_conn, &md5).ok().flatten().and_then(|b| b.file_content)
                            } else {
                                None
                            };

                            let (match_seed, effective_nonce) = match seed_override {
                                Some(seed) => (seed, 0u64),
                                None => ((wire_seed as u64).wrapping_add(1), run_nonce),
                            };

                            let osu_bytes = osu_content.as_deref().map(str::as_bytes);
                            let mut sim_results: Vec<(usize, simulator::SimulationResult)> = Vec::with_capacity(bots.len());
                            for (slot_idx, bot_uid, skill, eff_mods, cached_score) in &bots {
                                let res = if let Some(ref target_score) = cached_score {
                                    simulator::simulate_constrained_score(osu_bytes, target_score, mode, *slot_idx, match_seed)
                                } else {
                                    simulator::simulate_bot(
                                        osu_bytes,
                                        skill,
                                        mode,
                                        *eff_mods,
                                        win_condition,
                                        *slot_idx,
                                        *bot_uid,
                                        match_seed,
                                        effective_nonce,
                                    )
                                };
                                sim_results.push((*slot_idx, res));
                            }

                            let mut s = state.write().await;
                            let wire_out = if let Some(ref mut room) = s.active_match {
                                room.in_progress = true;
                                room.human_last_frame = None;
                                room.human_failed = false;
                                room.run_nonce = room.run_nonce.wrapping_add(1);

                                for slot in &mut room.slots {
                                    if matches!(slot.status, SlotStatus::NotReady | SlotStatus::Ready) {
                                        slot.status = SlotStatus::Playing;
                                    }
                                }
                                for (slot_idx, res) in sim_results {
                                    if let Some(ref mut bot) = room.slots[slot_idx].bot {
                                        bot.timeline = res.timeline;
                                        bot.fail_time_ms = res.fail_time_ms;
                                        bot.last_sent_idx = 0;
                                        bot.has_failed = false;
                                    }
                                }
                                Some(room.to_wire())
                            } else {
                                None
                            };
                            if let (Some(ref mut p), Some(wire)) = (&mut s.player, wire_out) {
                                p.queue.extend_from_slice(&packets::match_start(&wire));
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchLoadComplete as u16 => {
                        let mut s = state.write().await;
                        if let Some(ref mut p) = s.player {
                            p.queue.extend_from_slice(&packets::match_all_players_loaded());
                        }
                    }
                    x if x == packets::PacketId::OsuMatchSkipRequest as u16 => {
                        let mut s = state.write().await;
                        if let Some(ref mut p) = s.player {
                            p.queue.extend_from_slice(&packets::match_skip());
                        }
                    }
                    x if x == packets::PacketId::OsuMatchScoreUpdate as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(frame) = reader.read_score_frame() {
                            let current_time = frame.time;
                            let mut s = state.write().await;
                            let mut out_bytes = Vec::new();
                            if let Some(ref mut room) = s.active_match {
                                if room.in_progress {
                                    room.human_last_frame = Some(frame);
                                    for (slot_idx, slot) in room.slots.iter_mut().enumerate() {
                                        if slot.status != SlotStatus::Playing {
                                            continue;
                                        }
                                        if let Some(ref mut bot) = slot.bot {
                                            if !bot.has_failed && bot.fail_time_ms.is_some_and(|ft| ft <= current_time) {
                                                bot.has_failed = true;
                                                out_bytes.extend_from_slice(&packets::match_player_failed(slot_idx as i32));
                                            }
                                            if let Some(latest_idx) = bot.timeline.iter().rposition(|f| f.time <= current_time) {
                                                if latest_idx >= bot.last_sent_idx {
                                                    let mut out_frame = bot.timeline[latest_idx].clone();
                                                    out_frame.id = slot_idx as u8;
                                                    out_bytes.extend_from_slice(&packets::match_score_update(&out_frame));
                                                    bot.last_sent_idx = latest_idx + 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            if let Some(ref mut p) = s.player {
                                p.queue.extend_from_slice(&out_bytes);
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchFailed as u16 => {
                        let mut s = state.write().await;
                        let AppState { active_match, player, .. } = &mut *s;
                        if let (Some(ref mut room), Some(ref mut p)) = (active_match, player) {
                            if room.in_progress {
                                room.human_failed = true;
                                let all_bots_failed = room
                                    .slots
                                    .iter()
                                    .filter_map(|sl| sl.bot.as_ref())
                                    .all(|b| b.has_failed);
                                if all_bots_failed {
                                    finalize_match(room, p);
                                }
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuMatchComplete as u16 => {
                        let mut s = state.write().await;
                        let AppState { active_match, player, .. } = &mut *s;
                        if let (Some(ref mut room), Some(ref mut p)) = (active_match, player) {
                            if room.in_progress {
                                finalize_match(room, p);
                            }
                        }
                    }
                    x if x == packets::PacketId::OsuLogout as u16 => {
                        let mut s = state.write().await;
                        s.player = None;
                        s.pending_login_name = None;
                        s.active_match = None;
                        s.in_lobby = false;
                        utils::log(&format!("Player {} logged out.", player_name));
                    }
                    _ => {
                        if crate::logger::is_debug() {
                            crate::logger::debug(&format!("Unhandled Bancho packet ID: {} ({} bytes)", in_pkt.id, in_pkt.payload.len()));
                        }
                    }
                }
            }

            let mut s = state.write().await;
            let queue = if let Some(ref mut player) = s.player {
                if !player.queue.is_empty() {
                    player.clear_queue()
                } else {
                    Vec::new()
                }
            } else {
                Vec::new()
            };

            Response::new(queue)
        }
    }
}

async fn login(state: Arc<RwLock<AppState>>, body_bytes: &[u8]) -> (Vec<u8>, String) {
    if crate::logger::is_debug() {
        crate::logger::debug(&format!("Bancho login packet received ({} bytes)", body_bytes.len()));
    }
    let mut body = Vec::new();

    // Extract username primarily from body_bytes (authoritative login request)
    let mut name: Option<String> = None;
    if let Ok(body_str) = std::str::from_utf8(body_bytes) {
        let mut lines = body_str.lines();
        if let Some(first_line) = lines.next() {
            let u = first_line.trim();
            if !u.is_empty() {
                name = Some(u.to_string());
            }
        }
    }

    // Fallback to pending username from bancho_connect.php or pre-login if body_bytes is empty
    if name.is_none() {
        let s = state.read().await;
        name = s.pending_login_name.clone();
    }

    if name.is_none() {
        for _ in 0..2 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let s = state.read().await;
            if s.pending_login_name.is_some() {
                name = s.pending_login_name.clone();
                break;
            }
        }
    }

    let Some(profile_name) = name else {
        if crate::logger::is_debug() {
            crate::logger::debug("Bancho login failed: could not extract username from body or pending session.");
        }
        body.extend_from_slice(&packets::user_id(-5));
        body.extend_from_slice(&packets::notification("Please restart your game to login!"));
        utils::log("Player needs to restart game!");
        return (body, "fail".to_string());
    };

    {
        let mut s = state.write().await;
        s.pending_login_name = None;
    }

    let (api_key_opt, http_client, osu_uname_opt, config_country_opt, osu_daily_api_key_opt) = {
        let s = state.read().await;
        (s.config.osu_api_key.clone(), s.http.clone(), s.config.osu_username.clone(), s.config.country.clone(), s.config.osu_daily_api_key.clone())
    };

    let mut friend_records = {
        let s = state.read().await;
        let db = s.db.lock().await;
        let _ = db::ensure_profile(&db, &profile_name);
        let _ = db::ensure_avatar(&db, &profile_name);
        let _ = db::remove_friend(&db, &profile_name, 2);
        let _ = db::remove_friend(&db, &profile_name, 2070907);
        db::get_friend_records(&db, &profile_name).unwrap_or_default()
    };
    friend_records.retain(|f| f.friend_id > 2 && f.friend_id != 2070907);

    // Enqueue friend stats enrichment in the background so login packets are sent immediately
    if let Some(ref key) = api_key_opt {
        let friends_to_fetch: Vec<crate::db::FriendRecord> = friend_records
            .iter()
            .filter(|f| f.friend_id > 2 && f.friend_id != BANCHOBOT_ID && f.friend_id != TILLERINO_ID && f.friend_id != 2070907)
            .filter(|f| f.friend_name.is_empty() || (f.rank == 1 && f.pp == 0))
            .cloned()
            .collect();

        if !friends_to_fetch.is_empty() {
            let http_clone = http_client.clone();
            let key_clone = key.clone();
            let state_clone = Arc::clone(&state);
            let profile_name_clone = profile_name.clone();

            tokio::spawn(async move {
                for mut f in friends_to_fetch {
                    let query = if f.friend_id > 0 { f.friend_id.to_string() } else { f.friend_name.clone() };
                    if let Some(fetched) = utils::fetch_user_stats_from_api(&http_clone, &key_clone, &query).await {
                        f.friend_name = fetched.friend_name;
                        f.rank = fetched.rank;
                        f.pp = fetched.pp;
                        f.acc = fetched.acc;
                        f.country = fetched.country;
                        f.ranked_score = fetched.ranked_score;
                        f.total_score = fetched.total_score;
                        f.playcount = fetched.playcount;

                        {
                            let s = state_clone.read().await;
                            let db = s.db.lock().await;
                            let _ = db::save_friend_stats(&db, &profile_name_clone, &f);
                        }

                        let display_name = if f.friend_name.is_empty() { format!("Friend {}", f.friend_id) } else { f.friend_name.clone() };
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

                        let mut s = state_clone.write().await;
                        if let Some(ref mut p) = s.player {
                            p.queue.extend_from_slice(&packets::user_presence(&fp));
                            p.queue.extend_from_slice(&packets::user_stats(&fp));
                        }
                    }
                }
            });
        }
    }

    let mut friend_ids: Vec<i32> = friend_records.iter().map(|f| f.friend_id).filter(|&id| id > 2).collect();
    if !friend_ids.contains(&BANCHOBOT_ID) {
        friend_ids.push(BANCHOBOT_ID);
    }
    if !friend_ids.contains(&TILLERINO_ID) {
        friend_ids.push(TILLERINO_ID);
    }

    let mut player = Player::new(profile_name.clone());

    let db_country = {
        let s = state.read().await;
        let db = s.db.lock().await;
        db::get_profile_country(&db, &profile_name).unwrap_or(0)
    };

    if db_country != 0 {
        player.country = db_country;
    } else if let Some(ref c) = config_country_opt {
        let code_byte = utils::country_code_to_byte(c);
        player.country = code_byte;
        let s = state.read().await;
        let db = s.db.lock().await;
        let _ = db::save_profile_country(&db, &profile_name, code_byte);
    }

    if let Some(ref key) = api_key_opt {
        let player_query = osu_uname_opt.clone().unwrap_or_else(|| profile_name.clone());
        let http_clone = http_client.clone();
        let key_clone = key.clone();
        let state_clone = Arc::clone(&state);
        let profile_name_clone = profile_name.clone();
        tokio::spawn(async move {
            if let Some(fetched) = utils::fetch_user_stats_from_api(&http_clone, &key_clone, &player_query).await {
                if fetched.country != 0 {
                    let s = state_clone.read().await;
                    let db = s.db.lock().await;
                    let _ = db::save_profile_country(&db, &profile_name_clone, fetched.country);
                }
                let mut s = state_clone.write().await;
                if let Some(ref mut p) = s.player {
                    if fetched.country != 0 {
                        p.country = fetched.country;
                    }
                    if p.rank == 9999999 || p.rank <= 1 {
                        p.rank = fetched.rank;
                    }
                    p.queue.extend_from_slice(&packets::user_presence(p));
                    p.enqueue_stats();
                }
            }
        });
    }

    // Auto-sync medals asynchronously on login so historical or newly added medals trigger toast notifications
    {
        let state_medals_clone = Arc::clone(&state);
        let profile_medals_name = profile_name.clone();
        tokio::spawn(async move {
            let newly_earned = {
                let s = state_medals_clone.read().await;
                let conn = s.db.lock().await;
                crate::handlers::medals::retroactive_eval_profile(&conn, &profile_medals_name).unwrap_or_default()
            };
            if !newly_earned.is_empty() {
                let mut s = state_medals_clone.write().await;
                if let Some(ref mut p) = s.player {
                    for &id in &newly_earned {
                        if let Some(m) = crate::types::medal::get_medal_by_id(id) {
                            let toast = format!("Medal Unlocked: {}\n{}", m.name, m.description);
                            p.queue.extend_from_slice(&packets::notification(&toast));
                        }
                    }
                }
            }
        });
    }

    body.extend_from_slice(&packets::user_id(player.userid));
    body.extend_from_slice(&packets::notification("Welcome to local bancho! (^-^)/"));
    body.extend_from_slice(&packets::protocol_version(19));
    body.extend_from_slice(&packets::bancho_privs(player.bancho_privs));
    body.extend_from_slice(&packets::friends_list(&friend_ids));

    {
        let s = state.read().await;
        if let (Some(ref img), Some(ref click)) = (&s.config.menu_icon.image_link, &s.config.menu_icon.click_link) {
            body.extend_from_slice(&packets::menu_icon(img, click));
        }
    }

    let enable_recent = {
        let s = state.read().await;
        s.config.enable_recent_channel
    };

    for (cname, cdesc) in CHANNELS {
        if *cname == "#recent" && !enable_recent {
            continue;
        }
        body.extend_from_slice(&packets::channel_info(cname, cdesc, 1));
    }
    body.extend_from_slice(&packets::channel_info_end());
    for (cname, _) in CHANNELS {
        if *cname == "#recent" && !enable_recent {
            continue;
        }
        body.extend_from_slice(&packets::channel_join(cname));
    }

    {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, &profile_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, &profile_name).unwrap_or(0);
        player.calculate_stats(&scores, None, playcount);
    }

    if let Some(daily_key) = osu_daily_api_key_opt {
        let http_clone = http_client.clone();
        let pp = player.pp;
        let mode = player.mode;
        let state_clone = Arc::clone(&state);
        tokio::spawn(async move {
            if let Some(rank) = utils::get_rank_from_daily(&http_clone, &daily_key, pp, mode).await {
                let mut s = state_clone.write().await;
                if let Some(ref mut p) = s.player {
                    p.rank = rank;
                    p.queue.extend_from_slice(&packets::user_presence(p));
                    p.enqueue_stats();
                }
            }
        });
    }

    let player_id = player.userid;
    let mut all_ids = Vec::with_capacity(3 + friend_records.len());
    all_ids.push(player_id);
    all_ids.push(BANCHOBOT_ID);
    all_ids.push(TILLERINO_ID);
    for f in &friend_records {
        if f.friend_id > 2 && f.friend_id != player_id && f.friend_id != 2070907 && !all_ids.contains(&f.friend_id) {
            all_ids.push(f.friend_id);
        }
    }
    body.extend_from_slice(&packets::user_presence_bundle(&all_ids));

    body.extend_from_slice(&packets::user_presence(&player));
    body.extend_from_slice(&packets::user_stats(&player));

    let bancho = create_bot_player("BanchoBot", BANCHOBOT_ID);
    body.extend_from_slice(&packets::user_presence(&bancho));
    body.extend_from_slice(&packets::user_stats(&bancho));

    let tillerino = create_bot_player("Tillerino", TILLERINO_ID);
    body.extend_from_slice(&packets::user_presence(&tillerino));
    body.extend_from_slice(&packets::user_stats(&tillerino));

    for f in &friend_records {
        if f.friend_id <= 2 || f.friend_id == player_id || f.friend_id == BANCHOBOT_ID || f.friend_id == TILLERINO_ID || f.friend_id == 2070907 {
            continue;
        }
        let display_name = if f.friend_name.is_empty() { format!("Friend {}", f.friend_id) } else { f.friend_name.clone() };
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
        body.extend_from_slice(&packets::user_presence(&fp));
        body.extend_from_slice(&packets::user_stats(&fp));
    }

    body.extend_from_slice(&packets::local_message(
        "Welcome! BanchoBot and Tillerino are online. Type !help in chat or PM BanchoBot/Tillerino to see commands.",
        "#osu",
    ));

    utils::log_success(&format!("{} successfully logged in!", profile_name));

    {
        let mut s = state.write().await;
        s.player = Some(player);
    }

    (body, "success".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_login_packet_sequence_and_presence_order() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.pending_login_name = Some("SequenceTester".to_string());
        let shared_state = Arc::new(RwLock::new(app_state));

        let (body, status) = login(shared_state, b"").await;
        assert_eq!(status, "success");

        let pkts = packets::split_packets(&body);
        let _ids: Vec<u16> = pkts.iter().map(|p| p.id).collect();

        let friends_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoFriendsList as u16).expect("Friends list packet must be present");
        let mut reader = packets::PacketReader::new(friends_pkt.payload);
        let friends = reader.read_i32_list().expect("Valid friend list");
        assert!(friends.contains(&3), "Friend list must include BanchoBot (3) by default");
        assert!(friends.contains(&4), "Friend list must include Tillerino (4) by default");

        let bundle_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoUserPresenceBundle as u16).expect("Presence bundle must be present");
        let mut bundle_reader = packets::PacketReader::new(bundle_pkt.payload);
        let bundled_ids = bundle_reader.read_i32_list().expect("Valid bundled ids");
        assert!(bundled_ids.contains(&2), "Bundle must include local player");
        assert!(bundled_ids.contains(&3), "Bundle must include BanchoBot");
        assert!(bundled_ids.contains(&4), "Bundle must include Tillerino");

        let mut presence_user_ids = Vec::new();
        for p in &pkts {
            if p.id == packets::PacketId::ChoUserPresence as u16 {
                let mut r = packets::PacketReader::new(p.payload);
                let uid = r.read_i32().expect("Valid userid");
                presence_user_ids.push(uid);
            }
        }

        assert!(presence_user_ids.contains(&2), "Presence must include local player");
        assert!(presence_user_ids.contains(&3), "Presence must include BanchoBot");
        assert!(presence_user_ids.contains(&4), "Presence must include Tillerino");

        let notif_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoNotification as u16).expect("Notification packet present");
        let mut r_notif = packets::PacketReader::new(notif_pkt.payload);
        assert_eq!(r_notif.read_string().expect("notif string"), "Welcome to local bancho! (^-^)/");

        let last_msg_pkt = pkts.iter().rev().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("Welcome message packet present");
        let mut r = packets::PacketReader::new(last_msg_pkt.payload);
        let client = r.read_string().expect("client");
        let msg = r.read_string().expect("msg");
        let target = r.read_string().expect("target");
        let uid = r.read_i32().expect("uid");
        assert_eq!(client, "local");
        assert_eq!(target, "#osu");
        assert_eq!(uid, -1);
        assert!(msg.contains("BanchoBot and Tillerino are online"));
    }

    #[tokio::test]
    async fn test_login_recent_channel_toggle() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let config = crate::types::config::Config { enable_recent_channel: false, ..Default::default() };
        let mut app_state = AppState::new(conn, config);
        app_state.pending_login_name = Some("NoRecentTester".to_string());
        let shared_state = Arc::new(RwLock::new(app_state));

        let (body, status) = login(shared_state, b"").await;
        assert_eq!(status, "success");

        let pkts = packets::split_packets(&body);
        for p in &pkts {
            if p.id == packets::PacketId::ChoChannelInfo as u16 || p.id == packets::PacketId::ChoChannelJoinSuccess as u16 {
                let mut reader = packets::PacketReader::new(p.payload);
                let cname = reader.read_string().expect("cname");
                assert_ne!(cname, "#recent", "#recent channel must NOT be joined or listed when disabled");
            }
        }
    }

    #[tokio::test]
    async fn test_presence_request_does_not_loop_bundle() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "LocalHero").unwrap();

        let friend_rec = db::FriendRecord {
            friend_id: 7562902,
            friend_name: "mrekk".to_string(),
            rank: 1,
            pp: 32220,
            acc: 98.28,
            country: 16,
            ranked_score: 154251420480,
            total_score: 1047183973370,
            playcount: 242857,
        };
        db::save_friend_stats(&conn, "LocalHero", &friend_rec).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = Player::new("LocalHero".to_string());
        player.userid = 2;
        app_state.player = Some(player);
        let shared_state = Arc::new(RwLock::new(app_state));

        let pkt98 = vec![98, 0, 0, 0, 0, 0, 0];
        let resp98 = handle(shared_state.clone(), Some("valid-token"), &pkt98).await;
        let pkts98 = packets::split_packets(&resp98.body);
        let bundle_pkt = pkts98.iter().find(|p| p.id == packets::PacketId::ChoUserPresenceBundle as u16);
        assert!(bundle_pkt.is_some(), "Packet 98 must return presence bundle");

        let mut pkt97_payload = Vec::new();
        pkt97_payload.extend_from_slice(&1i16.to_le_bytes());
        pkt97_payload.extend_from_slice(&7562902i32.to_le_bytes());
        let mut pkt97 = vec![97, 0, 0];
        pkt97.extend_from_slice(&(pkt97_payload.len() as u32).to_le_bytes());
        pkt97.extend_from_slice(&pkt97_payload);

        let resp97 = handle(shared_state.clone(), Some("valid-token"), &pkt97).await;
        let pkts97 = packets::split_packets(&resp97.body);
        assert!(!pkts97.iter().any(|p| p.id == packets::PacketId::ChoUserPresenceBundle as u16), "Packet 97 must NOT send presence bundle");
        let presence97 = pkts97.iter().find(|p| p.id == packets::PacketId::ChoUserPresence as u16);
        assert!(presence97.is_some(), "Packet 97 must return presence for requested user");

        let mut pkt85_payload = Vec::new();
        pkt85_payload.extend_from_slice(&1i16.to_le_bytes());
        pkt85_payload.extend_from_slice(&7562902i32.to_le_bytes());
        let mut pkt85 = vec![85, 0, 0];
        pkt85.extend_from_slice(&(pkt85_payload.len() as u32).to_le_bytes());
        pkt85.extend_from_slice(&pkt85_payload);

        let resp85 = handle(shared_state.clone(), Some("valid-token"), &pkt85).await;
        let pkts85 = packets::split_packets(&resp85.body);
        assert!(!pkts85.iter().any(|p| p.id == packets::PacketId::ChoUserPresenceBundle as u16), "Packet 85 must NOT send presence bundle");
        let stats85 = pkts85.iter().find(|p| p.id == packets::PacketId::ChoUserStats as u16);
        assert!(stats85.is_some(), "Packet 85 must return stats for requested user");
    }

    #[test]
    fn test_create_bot_player_stats() {
        let tillerino = create_bot_player("Tillerino", TILLERINO_ID);
        assert_eq!(tillerino.userid, 4);
        assert_eq!(tillerino.country, 0);
        assert_eq!(tillerino.acc, 0.0);
        assert_eq!(tillerino.rank, 0);
        assert_eq!(tillerino.playcount, 0);
        assert_eq!(tillerino.ranked_score, 0);
        assert_eq!(tillerino.total_score, 0);
        assert_eq!(tillerino.info_text, "");
        assert_eq!(tillerino.bancho_privs, 1);

        let bancho = create_bot_player("BanchoBot", BANCHOBOT_ID);
        assert_eq!(bancho.userid, 3);
        assert_eq!(bancho.country, 0);
        assert_eq!(bancho.acc, 0.0);
        assert_eq!(bancho.rank, 0);
        assert_eq!(bancho.playcount, 0);
        assert_eq!(bancho.ranked_score, 0);
        assert_eq!(bancho.total_score, 0);
        assert_eq!(bancho.info_text, "");
        assert_eq!(bancho.bancho_privs, 1);
    }

    #[tokio::test]
    async fn test_add_tillerino_friend_packet_maps_to_bot() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(Player::new("AddTester".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let mut pkt73_payload = Vec::new();
        pkt73_payload.extend_from_slice(&4i32.to_le_bytes());
        let mut pkt73 = vec![73, 0, 0];
        pkt73.extend_from_slice(&(pkt73_payload.len() as u32).to_le_bytes());
        pkt73.extend_from_slice(&pkt73_payload);

        let resp = handle(shared_state.clone(), Some("valid-token"), &pkt73).await;
        assert_eq!(resp.status, 200);

        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friends(&db_conn, "AddTester").unwrap();
            assert_eq!(friends, vec![(4, "Tillerino".to_string())]);
        }

        let mut pkt73_official_payload = Vec::new();
        pkt73_official_payload.extend_from_slice(&2070907i32.to_le_bytes());
        let mut pkt73_official = vec![73, 0, 0];
        pkt73_official.extend_from_slice(&(pkt73_official_payload.len() as u32).to_le_bytes());
        pkt73_official.extend_from_slice(&pkt73_official_payload);

        let resp_off = handle(shared_state.clone(), Some("valid-token"), &pkt73_official).await;
        assert_eq!(resp_off.status, 200);

        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friends(&db_conn, "AddTester").unwrap();
            assert_eq!(friends, vec![(4, "Tillerino".to_string())]);
        }
    }

    #[tokio::test]
    async fn test_login_direct_body_bytes_fast_and_loads_friends() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "FastTester").unwrap();

        let friend_rec = db::FriendRecord {
            friend_id: 1001,
            friend_name: "QuickFriend".to_string(),
            rank: 42,
            pp: 5000,
            acc: 99.0,
            country: 16,
            ranked_score: 1000000,
            total_score: 2000000,
            playcount: 500,
        };
        db::save_friend_stats(&conn, "FastTester", &friend_rec).unwrap();

        let config = crate::types::config::Config::default();
        let app_state = AppState::new(conn, config);
        let shared_state = Arc::new(RwLock::new(app_state));

        let start = std::time::Instant::now();
        let (body, status) = login(shared_state, b"FastTester\npasshash\nversion").await;
        let elapsed = start.elapsed();

        assert_eq!(status, "success");
        assert!(elapsed < std::time::Duration::from_millis(150), "Login must not stall (took {:?})", elapsed);

        let pkts = packets::split_packets(&body);
        let friends_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoFriendsList as u16).expect("Friends list present");
        let mut reader = packets::PacketReader::new(friends_pkt.payload);
        let friends = reader.read_i32_list().expect("Valid friend list");
        assert!(friends.contains(&1001), "Cached friend must be in friends list immediately");
        assert!(friends.contains(&3), "BanchoBot must be in friends list");
        assert!(friends.contains(&4), "Tillerino must be in friends list");
    }

    #[tokio::test]
    async fn test_login_prioritizes_body_bytes_over_stale_pending_name() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "OldProfile").unwrap();
        db::ensure_profile(&conn, "NewProfile").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        // Simulate stale pending_login_name from previous leaderboard request
        app_state.pending_login_name = Some("OldProfile".to_string());
        let shared_state = Arc::new(RwLock::new(app_state));

        let (body, status) = login(shared_state.clone(), b"NewProfile\npasshash\nversion").await;
        assert_eq!(status, "success");

        let s = shared_state.read().await;
        let player = s.player.as_ref().expect("Player logged in");
        assert_eq!(player.name, "NewProfile", "Authoritative username in body_bytes must take precedence over stale pending_login_name");
        assert_eq!(s.pending_login_name, None, "pending_login_name must be cleared after login");

        let pkts = packets::split_packets(&body);
        let presence_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoUserPresence as u16).expect("Presence present");
        let mut reader = packets::PacketReader::new(presence_pkt.payload);
        let _uid = reader.read_i32().unwrap();
        let uname = reader.read_string().unwrap();
        assert_eq!(uname, "NewProfile");
    }

    #[tokio::test]
    async fn test_osu_logout_packet_clears_player_and_pending_name() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(Player::new("LoggedOutUser".to_string()));
        app_state.pending_login_name = Some("LoggedOutUser".to_string());
        let shared_state = Arc::new(RwLock::new(app_state));

        // Create OsuLogout packet (id 2)
        let logout_pkt = packets::write_packet(packets::PacketId::OsuLogout as u16, &packets::write_i32(0));
        let resp = handle(shared_state.clone(), Some("dummy-token"), &logout_pkt).await;
        assert_eq!(resp.status, 200);

        let s = shared_state.read().await;
        assert!(s.player.is_none(), "Player must be cleared on OsuLogout");
        assert!(s.pending_login_name.is_none(), "pending_login_name must be cleared on OsuLogout");
    }

    #[tokio::test]
    async fn test_login_loads_persisted_profile_country_immediately() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "GermanUser").unwrap();
        db::save_profile_country(&conn, "GermanUser", 56).unwrap(); // DE is 56

        let config = crate::types::config::Config::default();
        let app_state = AppState::new(conn, config);
        let shared_state = Arc::new(RwLock::new(app_state));

        let (body, status) = login(shared_state.clone(), b"GermanUser\npasshash\nversion").await;
        assert_eq!(status, "success");

        let s = shared_state.read().await;
        let player = s.player.as_ref().expect("Player logged in");
        assert_eq!(player.country, 56, "Saved profile country must be loaded immediately");

        let pkts = packets::split_packets(&body);
        let presence_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoUserPresence as u16).expect("Presence present");
        let mut reader = packets::PacketReader::new(presence_pkt.payload);
        let _uid = reader.read_i32().unwrap();
        let _uname = reader.read_string().unwrap();
        let _timezone = reader.read_u8().unwrap();
        let country = reader.read_u8().unwrap();
        assert_eq!(country, 56, "Initial login presence packet must carry the saved country immediately");
    }

    #[tokio::test]
    async fn test_mp_lifecycle_kick_via_lock_host_protection_and_friend_safe_logout() {
        use crate::multiplayer::bot::{BotProfile, BotSource, SkillProfile};

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "HostPlayer").unwrap();
        // Save real friend (mrekk) in DB so friend-safe logout skips logging him out
        let friend_rec = db::FriendRecord {
            friend_id: 7562902,
            friend_name: "mrekk".to_string(),
            rank: 1,
            pp: 30000,
            acc: 99.0,
            country: 16,
            ranked_score: 0,
            total_score: 0,
            playcount: 0,
        };
        db::save_friend_stats(&conn, "HostPlayer", &friend_rec).unwrap();

        let mut app_state = AppState::new(conn, crate::types::config::Config::default());
        app_state.player = Some(Player::new("HostPlayer".to_string()));
        let shared = Arc::new(RwLock::new(app_state));

        // 1. OsuJoinLobby
        let join_lobby = packets::write_packet(packets::PacketId::OsuJoinLobby as u16, &[]);
        let _ = handle(shared.clone(), Some("tok"), &join_lobby).await;
        assert!(shared.read().await.in_lobby);

        // 2. OsuCreateMatch
        let mut create_wire = packets::MatchWire::default();
        create_wire.name = "My Bot Room".to_string();
        create_wire.slot_status = [SlotStatus::Open as u8; 16];
        let create_pkt = packets::write_packet(
            packets::PacketId::OsuCreateMatch as u16,
            &packets::write_match_bytes(&create_wire, true),
        );
        let resp = handle(shared.clone(), Some("tok"), &create_pkt).await;
        let pkts = packets::split_packets(&resp.body);
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoMatchJoinSuccess as u16));
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoChannelJoinSuccess as u16));
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoSendMessage as u16));

        // Insert 2 bots into the room: slot 1 = synthetic bot, slot 2 = mimicked friend (7562902)
        let synth_id = {
            let mut s = shared.write().await;
            let room = s.active_match.as_mut().unwrap();
            let sid = room.alloc_synthetic_bot_id();
            let bot1 = BotProfile::from_preset("5digit", sid).unwrap();
            assert_eq!(room.add_bot(bot1).unwrap(), 1);
            let bot2 = BotProfile {
                userid: 7562902,
                name: "mrekk".to_string(),
                country: 16,
                rank: 1,
                pp: 30000,
                acc: 99.0,
                source: BotSource::MimicPlayer {
                    username: "mrekk".to_string(),
                    osu_user_id: Some(7562902),
                    fallback_skill: SkillProfile::default(),
                },
            };
            assert_eq!(room.add_bot(bot2).unwrap(), 2);
            sid
        };

        // 3. OsuUserPresenceRequest / OsuUserStatsRequest resolves synthetic bot
        let mut req_payload = Vec::new();
        req_payload.extend_from_slice(&1i16.to_le_bytes());
        req_payload.extend_from_slice(&synth_id.to_le_bytes());
        let pres_req = packets::write_packet(packets::PacketId::OsuUserPresenceRequest as u16, &req_payload);
        let stats_req = packets::write_packet(packets::PacketId::OsuUserStatsRequest as u16, &req_payload);
        let mut both_req = pres_req;
        both_req.extend_from_slice(&stats_req);
        let resp_req = handle(shared.clone(), Some("tok"), &both_req).await;
        let pkts_req = packets::split_packets(&resp_req.body);
        assert!(pkts_req.iter().any(|p| p.id == packets::PacketId::ChoUserPresence as u16));
        assert!(pkts_req.iter().any(|p| p.id == packets::PacketId::ChoUserStats as u16));

        // 4. OsuMatchChangeSettings (enable TeamVs + FreeMods)
        let mut settings_wire = shared.read().await.active_match.as_ref().unwrap().to_wire();
        settings_wire.team_type = TeamType::TeamVs as u8;
        settings_wire.freemods = 1;
        settings_wire.beatmap_id = 555;
        settings_wire.beatmap_md5 = "abc123".to_string();
        let settings_pkt = packets::write_packet(
            packets::PacketId::OsuMatchChangeSettings as u16,
            &packets::write_match_bytes(&settings_wire, true),
        );
        let resp_set = handle(shared.clone(), Some("tok"), &settings_pkt).await;
        let pkts_set = packets::split_packets(&resp_set.body);
        assert!(pkts_set.iter().any(|p| p.id == packets::PacketId::ChoUpdateMatch as u16));
        {
            let s = shared.read().await;
            let room = s.active_match.as_ref().unwrap();
            assert!(room.freemods);
            assert_eq!(room.team_type, TeamType::TeamVs);
            assert_ne!(room.slots[0].team, Team::Neutral);
            assert_ne!(room.slots[1].team, Team::Neutral);
        }

        // 5. OsuMatchTransferHost on bot slot 1 is blocked
        let transfer_pkt = packets::write_packet(packets::PacketId::OsuMatchTransferHost as u16, &packets::write_i32(1));
        let resp_tr = handle(shared.clone(), Some("tok"), &transfer_pkt).await;
        let pkts_tr = packets::split_packets(&resp_tr.body);
        assert!(pkts_tr.iter().any(|p| p.id == packets::PacketId::ChoSendMessage as u16));
        assert_eq!(shared.read().await.active_match.as_ref().unwrap().host_id, 2);

        // 6. OsuMatchLock on bot slot 1 evicts bot, locks slot, and logs out synthetic bot
        let lock_pkt = packets::write_packet(packets::PacketId::OsuMatchLock as u16, &packets::write_i32(1));
        let resp_lock = handle(shared.clone(), Some("tok"), &lock_pkt).await;
        let pkts_lock = packets::split_packets(&resp_lock.body);
        assert!(pkts_lock.iter().any(|p| p.id == packets::PacketId::ChoUpdateMatch as u16));
        let logged_out_on_lock: Vec<i32> = pkts_lock
            .iter()
            .filter(|p| p.id == packets::PacketId::ChoUserLogout as u16)
            .map(|p| packets::PacketReader::new(p.payload).read_i32().unwrap())
            .collect();
        assert_eq!(logged_out_on_lock, vec![synth_id]);
        {
            let s = shared.read().await;
            let room = s.active_match.as_ref().unwrap();
            assert_eq!(room.slots[1].status, SlotStatus::Locked);
            assert!(!room.slots[1].is_bot());
        }

        // 7. OsuPartMatch clears room, kicks #multiplayer, and does NOT log out friend 7562902
        let part_pkt = packets::write_packet(packets::PacketId::OsuPartMatch as u16, &[]);
        let resp_part = handle(shared.clone(), Some("tok"), &part_pkt).await;
        let pkts_part = packets::split_packets(&resp_part.body);
        assert!(pkts_part.iter().any(|p| p.id == packets::PacketId::ChoChannelKick as u16));
        assert!(pkts_part.iter().any(|p| p.id == packets::PacketId::ChoDisposeMatch as u16));
        let logged_out_on_part: Vec<i32> = pkts_part
            .iter()
            .filter(|p| p.id == packets::PacketId::ChoUserLogout as u16)
            .map(|p| packets::PacketReader::new(p.payload).read_i32().unwrap())
            .collect();
        assert!(
            !logged_out_on_part.contains(&7562902),
            "Must never log out a mimicked player who is in the friends list"
        );
        assert!(shared.read().await.active_match.is_none());
    }

    #[tokio::test]
    async fn test_mp_gameplay_loop_and_abort() {
        use crate::multiplayer::bot::BotProfile;

        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Gamer").unwrap();

        let mut app_state = AppState::new(conn, crate::types::config::Config::default());
        app_state.player = Some(Player::new("Gamer".to_string()));
        let mut wire = packets::MatchWire::default();
        wire.name = "Gameplay Room".to_string();
        wire.slot_status = [SlotStatus::Open as u8; 16];
        let mut room = MatchRoom::from_create_wire(1, 2, &wire);
        let b1_id = room.alloc_synthetic_bot_id();
        room.add_bot(BotProfile::from_preset("5digit", b1_id).unwrap()).unwrap();
        let b2_id = room.alloc_synthetic_bot_id();
        room.add_bot(BotProfile::from_preset("3digit", b2_id).unwrap()).unwrap();
        app_state.active_match = Some(room);
        let shared = Arc::new(RwLock::new(app_state));

        // 1. OsuMatchStart
        let start_pkt = packets::write_packet(packets::PacketId::OsuMatchStart as u16, &[]);
        let resp_start = handle(shared.clone(), Some("tok"), &start_pkt).await;
        let pkts_start = packets::split_packets(&resp_start.body);
        assert!(pkts_start.iter().any(|p| p.id == packets::PacketId::ChoMatchStart as u16));
        assert!(shared.read().await.active_match.as_ref().unwrap().in_progress);

        // 2. OsuMatchLoadComplete + OsuMatchSkipRequest
        let mut load_and_skip = packets::write_packet(packets::PacketId::OsuMatchLoadComplete as u16, &[]);
        load_and_skip.extend_from_slice(&packets::write_packet(packets::PacketId::OsuMatchSkipRequest as u16, &[]));
        let resp_ls = handle(shared.clone(), Some("tok"), &load_and_skip).await;
        let pkts_ls = packets::split_packets(&resp_ls.body);
        assert!(pkts_ls.iter().any(|p| p.id == packets::PacketId::ChoMatchAllPlayersLoaded as u16));
        assert!(pkts_ls.iter().any(|p| p.id == packets::PacketId::ChoMatchSkip as u16));

        // 3. Multiple OsuMatchScoreUpdate ticks
        let mut bot_updates_received = 0usize;
        for t in [5_000i32, 30_000, 75_000] {
            let human_frame = packets::ScoreFrame {
                time: t,
                id: 0,
                n300: (t / 200) as u16,
                total_score: t * 10,
                max_combo: (t / 200) as u16,
                current_combo: (t / 200) as u16,
                perfect: true,
                current_hp: 200,
                ..Default::default()
            };
            let score_pkt = packets::write_packet(
                packets::PacketId::OsuMatchScoreUpdate as u16,
                &packets::write_score_frame_bytes(&human_frame),
            );
            let resp_tick = handle(shared.clone(), Some("tok"), &score_pkt).await;
            let pkts_tick = packets::split_packets(&resp_tick.body);
            for p in pkts_tick {
                if p.id == packets::PacketId::ChoMatchScoreUpdate as u16 {
                    let decoded = packets::PacketReader::new(p.payload).read_score_frame().unwrap();
                    assert!(decoded.id == 1 || decoded.id == 2);
                    assert!(decoded.time <= t);
                    bot_updates_received += 1;
                }
            }
        }
        assert!(bot_updates_received >= 4, "Expected live bot ScoreFrame updates, got {}", bot_updates_received);

        // 4. OsuMatchComplete
        let comp_pkt = packets::write_packet(packets::PacketId::OsuMatchComplete as u16, &[]);
        let resp_comp = handle(shared.clone(), Some("tok"), &comp_pkt).await;
        let pkts_comp = packets::split_packets(&resp_comp.body);
        assert!(pkts_comp.iter().any(|p| p.id == packets::PacketId::ChoMatchComplete as u16));
        assert!(pkts_comp.iter().any(|p| p.id == packets::PacketId::ChoUpdateMatch as u16));
        assert!(pkts_comp.iter().any(|p| p.id == packets::PacketId::ChoSendMessage as u16));
        {
            let s = shared.read().await;
            let room = s.active_match.as_ref().unwrap();
            assert!(!room.in_progress);
            assert_eq!(room.slots[0].status, SlotStatus::NotReady);
            assert_eq!(room.slots[1].status, SlotStatus::Ready);
        }

        // 5. Start again and abort via !mp abort
        let _ = handle(shared.clone(), Some("tok"), &start_pkt).await;
        assert!(shared.read().await.active_match.as_ref().unwrap().in_progress);
        crate::multiplayer::commands::handle_mp_command(shared.clone(), "Gamer", "#multiplayer", &["abort"]).await;
        let s = shared.read().await;
        assert!(!s.active_match.as_ref().unwrap().in_progress);
        let pkts_abort = packets::split_packets(&s.player.as_ref().unwrap().queue);
        assert!(pkts_abort.iter().any(|p| p.id == packets::PacketId::ChoMatchAbort as u16));
    }
}
