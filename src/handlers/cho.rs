use crate::db;
use crate::packets;
use crate::server::response::Response;
use crate::state::AppState;
use crate::types::player::Player;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

pub const BANCHOBOT_ID: i32 = 3;
pub const TILLERINO_ID: i32 = 4;

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
                                if map_id > 0 {
                                    resolved_bmap = db::get_beatmap_by_id(&db_conn, map_id as i64).ok().flatten();
                                }
                                if resolved_bmap.is_none() && !map_md5.is_empty() {
                                    resolved_bmap = db::get_beatmap_by_md5(&db_conn, &map_md5).ok().flatten();
                                }
                            }

                            if resolved_bmap.is_none() {
                                let s = state.read().await;
                                if let Some(songs_dir) = utils::resolve_songs_folder(&s.config) {
                                    let mid = if map_id > 0 { Some(map_id as i64) } else { None };
                                    let md5 = if !map_md5.is_empty() { Some(map_md5.as_str()) } else { None };
                                    let hint = if !info_text.is_empty() { Some(info_text.as_str()) } else { None };
                                    if let Some((bmap, content)) = utils::find_and_parse_local_osu_file(&songs_dir, None, mid, md5, hint) {
                                        let db_conn = s.db.lock().await;
                                        let _ = db::insert_beatmap(&db_conn, &bmap);
                                        let _ = db::update_beatmap_file_content(&db_conn, &bmap.file_md5, &content);
                                        drop(db_conn);
                                        resolved_bmap = Some(bmap);
                                    }
                                }
                            }

                            let mut s = state.write().await;
                            if let Some(b) = resolved_bmap {
                                s.last_np_map = Some(b);
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
                            let mut all_ids = Vec::with_capacity(3 + friend_records.len());
                            all_ids.push(p.userid);
                            all_ids.push(BANCHOBOT_ID);
                            all_ids.push(TILLERINO_ID);
                            for f in &friend_records {
                                all_ids.push(f.friend_id);
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
                            if let Some(ref mut p) = s.player {
                                for target_id in requested_ids {
                                    if target_id == p.userid {
                                        p.queue.extend_from_slice(&packets::user_presence(p));
                                    } else if target_id == BANCHOBOT_ID {
                                        let bot = create_bot_player("BanchoBot", BANCHOBOT_ID);
                                        p.queue.extend_from_slice(&packets::user_presence(&bot));
                                    } else if target_id == TILLERINO_ID {
                                        let bot = create_bot_player("Tillerino", TILLERINO_ID);
                                        p.queue.extend_from_slice(&packets::user_presence(&bot));
                                    } else if let Some(f) = friend_records.iter().find(|fr| fr.friend_id == target_id) {
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
                    x if x == packets::PacketId::OsuUserStatsRequest as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(requested_ids) = reader.read_i32_list() {
                            let friend_records = {
                                let s = state.read().await;
                                let db_conn = s.db.lock().await;
                                db::get_friend_records(&db_conn, &player_name).unwrap_or_default()
                            };

                            let mut s = state.write().await;
                            if let Some(ref mut p) = s.player {
                                for target_id in requested_ids {
                                    if target_id == p.userid {
                                        p.enqueue_stats();
                                    } else if target_id == BANCHOBOT_ID {
                                        let bot = create_bot_player("BanchoBot", BANCHOBOT_ID);
                                        p.queue.extend_from_slice(&packets::user_stats(&bot));
                                    } else if target_id == TILLERINO_ID {
                                        let bot = create_bot_player("Tillerino", TILLERINO_ID);
                                        p.queue.extend_from_slice(&packets::user_stats(&bot));
                                    } else if let Some(f) = friend_records.iter().find(|fr| fr.friend_id == target_id) {
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
                    x if x == packets::PacketId::OsuFriendAdd as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(friend_id) = reader.read_i32() {
                            let (api_key, http) = {
                                let s = state.read().await;
                                (s.config.osu_api_key.clone(), s.http.clone())
                            };

                            let mut friend_rec =
                                if let Some(ref key) = api_key { utils::fetch_user_stats_from_api(&http, key, &friend_id.to_string()).await } else { None };

                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            let f_rec = friend_rec.take().unwrap_or_else(|| db::FriendRecord {
                                friend_id,
                                friend_name: format!("Friend {}", friend_id),
                                rank: 1,
                                pp: 0,
                                acc: 0.0,
                                country: 0,
                                ranked_score: 0,
                                total_score: 0,
                                playcount: 0,
                            });
                            let _ = db::save_friend_stats(&db_conn, &player_name, &f_rec);
                            let mut friend_ids = db::get_friend_ids(&db_conn, &player_name).unwrap_or_default();
                            if !friend_ids.contains(&3) {
                                friend_ids.push(3);
                            }
                            drop(db_conn);
                            drop(s);

                            let mut s = state.write().await;
                            if let Some(ref mut p) = s.player {
                                p.queue.extend_from_slice(&packets::friends_list(&friend_ids));

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
                    x if x == packets::PacketId::OsuFriendRemove as u16 => {
                        let mut reader = packets::PacketReader::new(in_pkt.payload);
                        if let Ok(friend_id) = reader.read_i32() {
                            let s = state.read().await;
                            let db_conn = s.db.lock().await;
                            let _ = db::remove_friend(&db_conn, &player_name, friend_id);
                            let mut friend_ids = db::get_friend_ids(&db_conn, &player_name).unwrap_or_default();
                            if !friend_ids.contains(&3) {
                                friend_ids.push(3);
                            }
                            drop(db_conn);
                            drop(s);

                            let mut s = state.write().await;
                            if let Some(ref mut p) = s.player {
                                p.queue.extend_from_slice(&packets::friends_list(&friend_ids));
                                p.queue.extend_from_slice(&packets::logout(friend_id));
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
                    _ => {}
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
    let mut body = Vec::new();

    // Synchronize pending username from bancho_connect.php
    let mut name: Option<String> = None;
    for _ in 0..5 {
        {
            let s = state.read().await;
            if s.pending_login_name.is_some() {
                name = s.pending_login_name.clone();
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    if name.is_none() {
        if let Ok(body_str) = std::str::from_utf8(body_bytes) {
            let mut lines = body_str.lines();
            if let Some(first_line) = lines.next() {
                let u = first_line.trim();
                if !u.is_empty() {
                    name = Some(u.to_string());
                }
            }
        }
    }

    let Some(profile_name) = name else {
        body.extend_from_slice(&packets::user_id(-5));
        body.extend_from_slice(&packets::notification("Please restart your game to login!"));
        utils::log("Player needs to restart game!");
        return (body, "fail".to_string());
    };

    {
        let mut s = state.write().await;
        s.pending_login_name = None;
    }

    let (api_key_opt, http_client, osu_uname_opt, config_country_opt) = {
        let s = state.read().await;
        (s.config.osu_api_key.clone(), s.http.clone(), s.config.osu_username.clone(), s.config.country.clone())
    };

    let mut friend_records = {
        let s = state.read().await;
        let db = s.db.lock().await;
        let _ = db::ensure_profile(&db, &profile_name);
        let _ = db::ensure_avatar(&db, &profile_name);
        db::get_friend_records(&db, &profile_name).unwrap_or_default()
    };

    if let Some(ref key) = api_key_opt {
        for f in &mut friend_records {
            if f.friend_id == 3 {
                continue;
            }
            let query = if f.friend_id > 0 { f.friend_id.to_string() } else { f.friend_name.clone() };
            if let Some(fetched) = utils::fetch_user_stats_from_api(&http_client, key, &query).await {
                f.friend_name = fetched.friend_name;
                f.rank = fetched.rank;
                f.pp = fetched.pp;
                f.acc = fetched.acc;
                f.country = fetched.country;
                f.ranked_score = fetched.ranked_score;
                f.total_score = fetched.total_score;
                f.playcount = fetched.playcount;

                let s = state.read().await;
                let db = s.db.lock().await;
                let _ = db::save_friend_stats(&db, &profile_name, f);
            }
        }
    }

    let mut friend_ids: Vec<i32> = friend_records.iter().map(|f| f.friend_id).collect();
    if !friend_ids.contains(&3) {
        friend_ids.push(3);
    }

    let mut player = Player::new(profile_name.clone());

    if let Some(ref c) = config_country_opt {
        player.country = utils::country_code_to_byte(c);
    } else if let Some(ref key) = api_key_opt {
        let player_query = osu_uname_opt.as_deref().unwrap_or(&profile_name);
        if let Some(fetched) = utils::fetch_user_stats_from_api(&http_client, key, player_query).await {
            player.country = fetched.country;
            if player.rank == 9999999 {
                player.rank = fetched.rank;
            }
        }
    }

    body.extend_from_slice(&packets::user_id(player.userid));
    body.extend_from_slice(&packets::notification("Welcome to the local osu! server (Rust edition)!\nSave replays to submit scores."));
    body.extend_from_slice(&packets::protocol_version(19));
    body.extend_from_slice(&packets::bancho_privs(player.bancho_privs));
    body.extend_from_slice(&packets::friends_list(&friend_ids));

    {
        let s = state.read().await;
        if let (Some(ref img), Some(ref click)) = (&s.config.menu_icon.image_link, &s.config.menu_icon.click_link) {
            body.extend_from_slice(&packets::menu_icon(img, click));
        }
    }

    for (cname, cdesc) in CHANNELS {
        body.extend_from_slice(&packets::channel_info(cname, cdesc, 1));
    }
    body.extend_from_slice(&packets::channel_info_end());
    for (cname, _) in CHANNELS {
        body.extend_from_slice(&packets::channel_join(cname));
    }

    {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_ranked_scores(&db_conn, &profile_name).unwrap_or_default();
        let playcount = db::get_playcount(&db_conn, &profile_name).unwrap_or(0);
        player.calculate_stats(&scores, None, playcount);

        if let Some(ref api_key) = s.config.osu_daily_api_key {
            if let Some(rank) = utils::get_rank_from_daily(&s.http, api_key, player.pp, player.mode).await {
                player.rank = rank;
            }
        }
    }

    let mut all_ids = Vec::with_capacity(3 + friend_records.len());
    all_ids.push(player.userid);
    all_ids.push(BANCHOBOT_ID);
    all_ids.push(TILLERINO_ID);
    for f in &friend_records {
        if !all_ids.contains(&f.friend_id) {
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

    body.extend_from_slice(&packets::local_message("Welcome! BanchoBot and Tillerino are online. Type !help in chat or PM BanchoBot/Tillerino to see commands.", "#osu"));

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
}
