use crate::handlers::banchobot;
use crate::handlers::tillerino;
use crate::state::AppState;
use std::sync::Arc;
use tokio::sync::RwLock;

#[allow(unused_imports)]
pub use crate::handlers::banchobot::reply as reply_banchobot;
#[allow(unused_imports)]
pub use crate::handlers::tillerino::{
    calculate_np_breakdown, format_np_reply, parse_np_message, reply as reply_tillerino, NpBreakdown, NpInfo,
};

/// The channel every player is in, and therefore where an answer to a command is shown when the
/// client did not say which channel it was typed in.
const PUBLIC_CHANNEL: &str = "#osu";

pub async fn handle_chat_message(state: Arc<RwLock<AppState>>, player_name: &str, message: &str, target: &str) {
    let trimmed = message.trim();

    let is_tillerino = target.eq_ignore_ascii_case(tillerino::BOT_NAME);
    let is_banchobot = target.eq_ignore_ascii_case(banchobot::BOT_NAME);
    let mut is_pm = is_tillerino || is_banchobot || !target.starts_with('#');
    let mut reply_target = if is_pm { player_name.to_string() } else { target.to_string() };

    let prefix = {
        let s = state.read().await;
        s.config.command_prefix.clone()
    };

    // Not every client sends `!cmd` as a message. The 2022 builds treat a leading `!` as a
    // private-message shortcut, so the text arrives with an *empty* message and the command in
    // the target field ("", "!help"). Left alone that is a PM to a user called "!help" and the
    // command is silently dropped, which is why nothing happened in chat. Treat that shape as the
    // command it obviously is, and answer in the channel the player is typing in rather than in a
    // PM tab they may not be looking at.
    let command_in_target = trimmed.is_empty() && !is_tillerino && !is_banchobot && (target.starts_with(&prefix) || target.starts_with('/'));
    let effective_message = if command_in_target { target } else { trimmed };

    if !is_banchobot && !command_in_target {
        if let Some(np_info) = parse_np_message(message) {
            tillerino::handle_np(&state, player_name, &reply_target, np_info).await;
            return;
        }
    } else if let Some(np_info) = parse_np_message(message) {
        tillerino::handle_np(&state, player_name, "", np_info).await;
        return;
    }

    if effective_message.starts_with('\x01') {
        return;
    }

    if command_in_target {
        is_pm = false;
        reply_target = PUBLIC_CHANNEL.to_string();
        crate::utils::log(&format!("{} ran `{}` (sent as a private-message target by the client)", player_name, target));
    }

    if effective_message.is_empty() {
        return;
    }

    let cmd_text = if let Some(stripped) = effective_message.strip_prefix(&prefix) {
        stripped
    } else if command_in_target {
        // The client used a slash here, since the command came from the target field.
        effective_message.strip_prefix('/').unwrap_or(effective_message)
    } else if is_pm {
        effective_message.strip_prefix('!').or_else(|| effective_message.strip_prefix('/')).unwrap_or(effective_message)
    } else {
        return;
    };

    let parts: Vec<&str> = cmd_text.split_whitespace().collect();
    if parts.is_empty() {
        return;
    }

    let cmd = parts[0].to_lowercase();
    let args = &parts[1..];

    let is_restricted = {
        let s = state.read().await;
        s.player.as_ref().map_or(false, |p| p.is_restricted)
    };

    if is_restricted {
        let is_unrestrict_cmd = matches!(
            cmd.as_str(),
            "restrictself" | "restrict" | "unrestrictself" | "unrestrict" | "help" | "commands"
        );

        if !is_unrestrict_cmd {
            if is_tillerino {
                banchobot::reply(&state, player_name, "Your account is currently in restricted mode! Cannot message other users.").await;
                return;
            } else if !is_pm {
                banchobot::reply(&state, player_name, "Your account is currently in restricted mode! Chatting in public channels is disabled.").await;
                return;
            }
        }
    }

    if is_tillerino {
        match cmd.as_str() {
            "clearscores" | "clearmap" | "removescores" | "deletescores" | "clearscore" | "removemap" | "clear" => {
                banchobot::handle_clear_scores(&state, player_name, &reply_target, args).await;
                return;
            }
            _ => {}
        }
        tillerino::handle_command(&state, player_name, &reply_target, &cmd, args).await;
        return;
    }

    if is_banchobot {
        banchobot::handle_command(&state, player_name, &reply_target, &cmd, args).await;
        return;
    }

    match cmd.as_str() {
        "r" | "recommend" | "with" | "acc" => {
            tillerino::handle_command(&state, player_name, &reply_target, &cmd, args).await;
        }
        _ => {
            banchobot::handle_command(&state, player_name, &reply_target, &cmd, args).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::packets;
    use crate::types::beatmap::Beatmap;
    use crate::types::mods::Mods;
    use crate::utils;

    #[tokio::test]
    async fn test_command_sent_as_private_message_target_is_answered_in_channel() {
        // osu! 2022 builds treat a leading `!` as a private-message shortcut: typing `!help` in
        // #osu arrives as an empty message with "!help" as the target. It has to run as a command
        // and be answered in #osu, not be dropped as a PM to a user called "!help".
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Box").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Box".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        handle_chat_message(shared_state.clone(), "Box", "", "!help").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let queue = p.clear_queue();
            let pkts = packets::split_packets(&queue);
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("command sent as target must still be answered");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            assert_eq!(r.read_string().unwrap(), "BanchoBot");
            let msg = r.read_string().unwrap();
            let target = r.read_string().unwrap();
            assert_eq!(target, "#osu", "the answer must land in the channel the player is typing in");
            assert!(msg.contains("BanchoBot Commands:"), "got: {}", msg);
        }

        // The same shape with a slash-prefixed command.
        handle_chat_message(shared_state.clone(), "Box", "", "/commands").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let queue = p.clear_queue();
            let pkts = packets::split_packets(&queue);
            assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoSendMessage as u16), "/commands sent as a target must be answered too");
        }
    }

    #[tokio::test]
    async fn test_normal_chat_and_command_shapes_still_work() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Box").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("Box".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        // A command sent the normal way, in a channel.
        handle_chat_message(shared_state.clone(), "Box", "!help", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let queue = p.clear_queue();
            let pkts = packets::split_packets(&queue);
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("in-channel command");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            r.read_string().unwrap();
            assert!(r.read_string().unwrap().contains("BanchoBot Commands:"));
            assert_eq!(r.read_string().unwrap(), "#osu");
        }

        // Ordinary chat is not a command and must produce no reply.
        handle_chat_message(shared_state.clone(), "Box", "hello", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let queue = p.clear_queue();
            let pkts = packets::split_packets(&queue);
            assert!(!pkts.iter().any(|p| p.id == packets::PacketId::ChoSendMessage as u16), "plain chat must not trigger a command");
        }

        // A PM to BanchoBot is still a PM, so the answer goes to the player.
        handle_chat_message(shared_state.clone(), "Box", "!help", "BanchoBot").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let queue = p.clear_queue();
            let pkts = packets::split_packets(&queue);
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("PM command");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            r.read_string().unwrap();
            r.read_string().unwrap();
            assert_eq!(r.read_string().unwrap(), "Box");
        }
    }

    #[test]
    fn test_parse_np_message_variants() {
        assert!(parse_np_message("/np").is_some());
        assert!(parse_np_message("!np").is_some());
        assert!(parse_np_message("/np ").is_some());
        assert!(parse_np_message("hello").is_none());

        let action = parse_np_message("\x01ACTION is listening to [https://osu.ppy.sh/b/123456 Artist - Title [Insane]]\x01").unwrap();
        assert_eq!(action.map_id, Some(123456));
        assert_eq!(action.mods, None);

        let playing = parse_np_message("\x01ACTION is playing [https://osu.ppy.sh/b/98765 Artist - Title [Extra]] <+HDDT>\x01").unwrap();
        assert_eq!(playing.map_id, Some(98765));
        assert_eq!(playing.mods, Some((Mods::HIDDEN | Mods::DOUBLETIME).bits()));

        let ce = parse_np_message("\x01ACTION is listening to [https://osu.ppy.sh/beatmapsets/1222729#osu/2543274 VINXIS - Sidetracked Day GAMMA]\x01").unwrap();
        assert_eq!(ce.map_id, Some(2543274));
        assert_eq!(ce.set_id, Some(1222729));
        assert_eq!(ce.title_hint.as_deref(), Some("VINXIS - Sidetracked Day GAMMA"));

        let plain = parse_np_message("*w is listening to VINXIS - Sidetracked Day GAMMA").unwrap();
        assert_eq!(plain.map_id, None);
        assert_eq!(plain.title_hint.as_deref(), Some("VINXIS - Sidetracked Day GAMMA"));

        let edit = parse_np_message("\x01ACTION is editing [https://osu.ppy.sh/b/54321 MapArtist - EditTitle [Insane]] +HD\x01").unwrap();
        assert_eq!(edit.map_id, Some(54321));
        assert_eq!(edit.mods, Some(Mods::HIDDEN.bits()));

        let slash = parse_np_message("\x01ACTION is listening to [https://osu.ppy.sh/beatmapsets/123456/789012 Artist - SlashTitle [Hard]]\x01").unwrap();
        assert_eq!(slash.set_id, Some(123456));
        assert_eq!(slash.map_id, Some(789012));
    }

    #[test]
    fn test_format_np_reply() {
        let b = NpBreakdown {
            map_id: 123,
            artist: "Artist".to_string(),
            title: "Title".to_string(),
            version: "Version".to_string(),
            mods_str: "+HD".to_string(),
            stars: 5.25,
            max_combo: 1000,
            ar: 9.3,
            od: 8.5,
            pp95: 150.0,
            pp98: 180.0,
            pp99: 195.0,
            pp100: 210.0,
        };
        let rep = format_np_reply(&b);
        assert!(rep.contains("https://osu.ppy.sh/b/123"));
        assert!(rep.contains("+HD"));
        assert!(rep.contains("210pp"));
    }

    #[test]
    fn test_calculate_np_breakdown_with_sample_map() {
        let osu_content = "osu file format v14\n\
            [General]\nMode: 0\n\
            [Metadata]\nTitle:Sample\nArtist:Sample\nCreator:Sample\nVersion:Normal\nBeatmapID:999\nBeatmapSetID:888\n\
            [Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:8\nApproachRate:9\nSliderMultiplier:1.4\nSliderTickRate:1\n\
            [TimingPoints]\n0,500,4,2,0,100,1,0\n\
            [HitObjects]\n\
            256,192,1000,1,0,0:0:0:0:\n\
            300,200,2000,1,0,0:0:0:0:\n";

        let mut bmap = Beatmap::blank();
        bmap.beatmap_id = 999;
        bmap.beatmapset_id = 888;
        bmap.artist = "Sample".to_string();
        bmap.title = "Sample".to_string();
        bmap.version = "Normal".to_string();
        bmap.mode = 0;

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

        handle_chat_message(shared_state.clone(), "PlayerTest", "!help", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let packets = packets::split_packets(&q);
            assert!(!packets.is_empty());
            assert_eq!(packets[0].id, packets::PacketId::ChoSendMessage as u16);
            let mut r = packets::PacketReader::new(packets[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(!msg.contains("/np : Tillerino PP calculations for current song"));
            assert!(msg.contains("BanchoBot Commands:"));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!mode rx", "#osu").await;
        {
            let s = shared_state.read().await;
            assert_eq!(s.mode, Some(Mods::RELAX));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!rank 5555", "#osu").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let loaded = db::get_beatmap_by_id(&db_conn, 5555).unwrap().unwrap();
            assert_eq!(loaded.approved, 1);
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!friend add 9988", "BanchoBot").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friend_ids(&db_conn, "PlayerTest").unwrap();
            assert!(friends.contains(&9988));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!friend add Tillerino", "BanchoBot").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friends(&db_conn, "PlayerTest").unwrap();
            assert!(friends.iter().any(|(id, name)| *id == 4 && name == "Tillerino"));
            // Ensure friend packet queued for player has bot IDs
            let p = s.player.as_ref().unwrap();
            assert!(!p.queue.is_empty());
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!friend add 2070907", "BanchoBot").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friends(&db_conn, "PlayerTest").unwrap();
            // Should map to 4, not insert 2070907
            assert!(friends.iter().any(|(id, name)| *id == 4 && name == "Tillerino"));
            assert!(!friends.iter().any(|(id, _)| *id == 2070907));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!friend add BanchoBot", "BanchoBot").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friends(&db_conn, "PlayerTest").unwrap();
            assert!(friends.iter().any(|(id, name)| *id == 3 && name == "BanchoBot"));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!friend remove Tillerino", "BanchoBot").await;
        {
            let s = shared_state.read().await;
            let db_conn = s.db.lock().await;
            let friends = db::get_friend_ids(&db_conn, "PlayerTest").unwrap();
            assert!(!friends.contains(&4));
            assert!(!friends.contains(&2070907));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!country DE", "#osu").await;
        {
            let s = shared_state.read().await;
            assert_eq!(s.player.as_ref().unwrap().country, 56);
        }
    }

    #[tokio::test]
    async fn test_tillerino_pm_and_recommendation() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "PlayerTest").unwrap();

        let osu_content = "osu file format v14\n\
            [General]\nMode: 0\n\
            [Metadata]\nTitle:TillerinoTest\nArtist:BotArtist\nCreator:Mapper\nVersion:Hard\nBeatmapID:10101\nBeatmapSetID:20202\n\
            [Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:8\nApproachRate:9\nSliderMultiplier:1.4\nSliderTickRate:1\n\
            [TimingPoints]\n0,500,4,2,0,100,1,0\n\
            [HitObjects]\n\
            256,192,1000,1,0,0:0:0:0:\n\
            300,200,2000,1,0,0:0:0:0:\n";

        let mut bmap = utils::parse_osu_file_to_beatmap(osu_content, Some(10101), Some(20202)).unwrap();
        bmap.file_content = Some(osu_content.to_string());
        db::insert_beatmap(&conn, &bmap).unwrap();
        db::update_beatmap_file_content(&conn, &bmap.file_md5, osu_content).unwrap();

        let temp_dir = std::env::temp_dir().join("test_empty_songs_dir");
        let _ = std::fs::create_dir_all(&temp_dir);
        let mut config = crate::types::config::Config::default();
        config.paths.songs = Some(temp_dir.to_str().unwrap().to_string());
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("PlayerTest".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        handle_chat_message(shared_state.clone(), "PlayerTest", "!help", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            let _target = r.read_string().unwrap();
            let sender_id = r.read_i32().unwrap();
            assert_eq!(sender, "Tillerino");
            assert_eq!(sender_id, 4);
            assert!(msg.contains("Tillerino Commands"));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!r", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            let _target = r.read_string().unwrap();
            let sender_id = r.read_i32().unwrap();
            assert_eq!(sender, "Tillerino");
            assert_eq!(sender_id, 4);
            assert!(msg.contains("TillerinoTest"));
            assert!(msg.contains("100%:"));
            assert!(s.last_np_map.is_some());
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!with HD", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            let _target = r.read_string().unwrap();
            let sender_id = r.read_i32().unwrap();
            assert_eq!(sender, "Tillerino");
            assert_eq!(sender_id, 4);
            assert!(msg.contains("+HD"));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "!acc 99", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            let _target = r.read_string().unwrap();
            let sender_id = r.read_i32().unwrap();
            assert_eq!(sender, "Tillerino");
            assert_eq!(sender_id, 4);
            assert!(msg.contains("99.00%:"));
        }

        handle_chat_message(shared_state.clone(), "PlayerTest", "/np", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let sender = r.read_string().unwrap();
            let _msg = r.read_string().unwrap();
            let _target = r.read_string().unwrap();
            let sender_id = r.read_i32().unwrap();
            assert_eq!(sender, "Tillerino");
            assert_eq!(sender_id, 4);
        }
    }

    #[tokio::test]
    async fn test_tillerino_np_switch_map() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "PlayerNP").unwrap();

        let map1_content = "osu file format v14\n\
            [General]\nMode: 0\n\
            [Metadata]\nTitle:FirstMap\nArtist:Artist1\nCreator:Mapper\nVersion:Normal\nBeatmapID:1001\nBeatmapSetID:2001\n\
            [Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:8\nApproachRate:9\nSliderMultiplier:1.4\nSliderTickRate:1\n\
            [TimingPoints]\n0,500,4,2,0,100,1,0\n\
            [HitObjects]\n\
            256,192,1000,1,0,0:0:0:0:\n\
            300,200,2000,1,0,0:0:0:0:\n";

        let map2_content = "osu file format v14\n\
            [General]\nMode: 0\n\
            [Metadata]\nTitle:SecondMap\nArtist:Artist2\nCreator:Mapper\nVersion:Hard\nBeatmapID:1002\nBeatmapSetID:2002\n\
            [Difficulty]\nHPDrainRate:6\nCircleSize:4.5\nOverallDifficulty:8.5\nApproachRate:9.3\nSliderMultiplier:1.5\nSliderTickRate:1\n\
            [TimingPoints]\n0,400,4,2,0,100,1,0\n\
            [HitObjects]\n\
            256,192,1000,1,0,0:0:0:0:\n\
            300,200,2000,1,0,0:0:0:0:\n";

        let mut bmap1 = utils::parse_osu_file_to_beatmap(map1_content, Some(1001), Some(2001)).unwrap();
        bmap1.file_content = Some(map1_content.to_string());
        db::insert_beatmap(&conn, &bmap1).unwrap();
        db::update_beatmap_file_content(&conn, &bmap1.file_md5, map1_content).unwrap();

        let mut bmap2 = utils::parse_osu_file_to_beatmap(map2_content, Some(1002), Some(2002)).unwrap();
        bmap2.file_content = Some(map2_content.to_string());
        db::insert_beatmap(&conn, &bmap2).unwrap();
        db::update_beatmap_file_content(&conn, &bmap2.file_md5, map2_content).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("PlayerNP".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        let np1 = "\x01ACTION is listening to [https://osu.ppy.sh/b/1001 Artist1 - FirstMap [Normal]]\x01";
        handle_chat_message(shared_state.clone(), "PlayerNP", np1, "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("FirstMap"), "Must calculate first map");
            assert_eq!(s.last_np_map.as_ref().unwrap().beatmap_id, 1001);
        }

        let np2 = "\x01ACTION is listening to [https://osu.ppy.sh/b/1002 Artist2 - SecondMap [Hard]]\x01";
        handle_chat_message(shared_state.clone(), "PlayerNP", np2, "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("SecondMap"), "Must switch to and calculate second map! Got: {}", msg);
            assert_eq!(s.last_np_map.as_ref().unwrap().beatmap_id, 1002);
        }

        handle_chat_message(shared_state.clone(), "PlayerNP", "!with HR", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("SecondMap"), "Must calculate HR on second map");
            assert!(msg.contains("+HR"));
        }

        {
            let mut s = shared_state.write().await;
            if let Some(ref mut p) = s.player {
                p.map_id = 1001;
                p.map_md5 = bmap1.file_md5.clone();
                p.info_text = "Artist1 - FirstMap [Normal]".to_string();
            }
        }
        handle_chat_message(shared_state.clone(), "PlayerNP", "/np", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("FirstMap"), "Must switch back to first map via /np");
            assert_eq!(s.last_np_map.as_ref().unwrap().beatmap_id, 1001);
        }
    }

    #[tokio::test]
    async fn test_banchobot_does_not_execute_tillerino_np() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "PlayerBancho").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("PlayerBancho".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        // Sending /np to BanchoBot should NOT invoke Tillerino
        handle_chat_message(shared_state.clone(), "PlayerBancho", "/np", "BanchoBot").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            if !pkts.is_empty() {
                let mut r = packets::PacketReader::new(pkts[0].payload);
                let sender = r.read_string().unwrap();
                assert_ne!(sender, "Tillerino", "Tillerino must not respond to BanchoBot messages");
            }
        }

        // Sending CTCP action to BanchoBot should also NOT invoke Tillerino
        let action = "\x01ACTION is listening to [https://osu.ppy.sh/b/1001 Artist - Title [Diff]]\x01";
        handle_chat_message(shared_state.clone(), "PlayerBancho", action, "BanchoBot").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            assert!(q.is_empty(), "BanchoBot should not respond to unhandled CTCP actions");
        }

        // Sending !r to BanchoBot invokes BanchoBot's recent command (sender is BanchoBot)
        handle_chat_message(shared_state.clone(), "PlayerBancho", "!r", "BanchoBot").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let sender = r.read_string().unwrap();
            assert_eq!(sender, "BanchoBot", "!r in PM to BanchoBot should be handled by BanchoBot");
        }
    }

    #[tokio::test]
    async fn test_chat_message_recalculate() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "PlayerRecalc").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let player = crate::types::player::Player::new("PlayerRecalc".to_string());
        app_state.player = Some(player);

        let shared_state = Arc::new(RwLock::new(app_state));

        // Test in channel (#osu)
        handle_chat_message(shared_state.clone(), "PlayerRecalc", "!recalculate", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoUserStats as u16));
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("Message packet not found");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            let sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert_eq!(sender, "BanchoBot");
            assert!(msg.contains("Profile recalculation complete!"));
        }

        // Test in PM to BanchoBot
        handle_chat_message(shared_state.clone(), "PlayerRecalc", "!recalc", "BanchoBot").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoUserStats as u16));
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("Message packet not found");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            let sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert_eq!(sender, "BanchoBot");
            assert!(msg.contains("Profile recalculation complete!"));
        }
    }

    #[tokio::test]
    async fn test_restricted_mode_chat_blocks_public_but_allows_unrestrict() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "RestrictedChatUser").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("RestrictedChatUser".to_string());
        player.is_restricted = true;
        app_state.player = Some(player);

        let shared_state = Arc::new(RwLock::new(app_state));

        handle_chat_message(shared_state.clone(), "RestrictedChatUser", "!roll", "#osu").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty(), "Must reply to user with restriction notice");
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("Message packet not found");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("restricted mode"), "Must notify about restricted mode blocking public chat");
        }

        handle_chat_message(shared_state.clone(), "RestrictedChatUser", "!r", "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty(), "Must reply with restriction warning");
            let msg_pkt = pkts.iter().find(|p| p.id == packets::PacketId::ChoSendMessage as u16).expect("Message packet not found");
            let mut r = packets::PacketReader::new(msg_pkt.payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("Cannot message other users"));
        }

        handle_chat_message(shared_state.clone(), "RestrictedChatUser", "!unrestrict", "#osu").await;
        {
            let s = shared_state.read().await;
            let p = s.player.as_ref().unwrap();
            assert!(!p.is_restricted, "Player must now be unrestricted");
        }
    }

    #[test]
    fn test_parse_np_message_action_without_link_and_angle_mods() {
        let unlinked = parse_np_message("\x01ACTION is listening to Artist - Song Title [Hard Diff]\x01").unwrap();
        assert_eq!(unlinked.map_id, None);
        assert_eq!(unlinked.set_id, None);
        assert_eq!(unlinked.title_hint.as_deref(), Some("Artist - Song Title [Hard Diff]"));
        assert_eq!(unlinked.mods, None);

        let angle_mods = parse_np_message("\x01ACTION is playing Artist - Song Title [Hard Diff] <+HDDT>\x01").unwrap();
        assert_eq!(angle_mods.title_hint.as_deref(), Some("Artist - Song Title [Hard Diff]"));
        assert_eq!(angle_mods.mods, Some((Mods::HIDDEN | Mods::DOUBLETIME).bits()));

        let text_mods = parse_np_message("\x01ACTION is listening to [https://osu.ppy.sh/s/12345 Artist - Song Title [Insane]] <Hidden, HardRock>\x01").unwrap();
        assert_eq!(text_mods.set_id, Some(12345));
        assert_eq!(text_mods.title_hint.as_deref(), Some("Artist - Song Title [Insane]"));
        assert_eq!(text_mods.mods, Some((Mods::HIDDEN | Mods::HARDROCK).bits()));
    }

    #[test]
    fn test_scan_dir_multidiff_matches_exact_difficulty() {
        let temp_dir = std::env::temp_dir().join(format!("osu_test_multidiff_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let set_dir = temp_dir.join("555 Composer - Multi Song");
        std::fs::create_dir_all(&set_dir).unwrap();

        for (diff, id) in &[("Easy", 101), ("Normal", 102), ("Hard", 103), ("Insane", 104)] {
            let content = format!(
                "osu file format v14\n\n[Metadata]\nTitle:Multi Song\nArtist:Composer\nCreator:Mapper\nVersion:{}\nBeatmapID:{}\nBeatmapSetID:555\n[Difficulty]\nHPDrainRate:5\nCircleSize:4\nOverallDifficulty:7\nApproachRate:8\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n",
                diff, id
            );
            let path = set_dir.join(format!("Composer - Multi Song (Mapper) [{}].osu", diff));
            std::fs::write(&path, content).unwrap();
        }

        // Querying for "Hard" must return Hard, NOT Easy (first in dir)
        let res_hard = utils::find_and_parse_local_osu_file(&temp_dir, Some(555), None, None, Some("Composer - Multi Song [Hard]"));
        assert!(res_hard.is_some());
        let (bmap_hard, _) = res_hard.unwrap();
        assert_eq!(bmap_hard.version, "Hard", "Must pick Hard difficulty, not the first file in dir");
        assert_eq!(bmap_hard.beatmap_id, 103);

        // Querying for "Insane" must return Insane
        let res_insane = utils::find_and_parse_local_osu_file(&temp_dir, Some(555), None, None, Some("Composer - Multi Song [Insane]"));
        assert!(res_insane.is_some());
        let (bmap_insane, _) = res_insane.unwrap();
        assert_eq!(bmap_insane.version, "Insane");
        assert_eq!(bmap_insane.beatmap_id, 104);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[tokio::test]
    async fn test_tillerino_np_same_set_diff_switch() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "PlayerSetSwitch").unwrap();

        let map_easy_content = "osu file format v14\n\n[Metadata]\nTitle:SetSong\nArtist:SetArtist\nCreator:Mapper\nVersion:Normal\nBeatmapID:7001\nBeatmapSetID:9000\n[Difficulty]\nHPDrainRate:3\nCircleSize:3\nOverallDifficulty:3\nApproachRate:4\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n";
        let mut bmap_easy = utils::parse_osu_file_to_beatmap(map_easy_content, Some(7001), Some(9000)).unwrap();
        bmap_easy.file_content = Some(map_easy_content.to_string());
        db::insert_beatmap(&conn, &bmap_easy).unwrap();
        db::update_beatmap_file_content(&conn, &bmap_easy.file_md5, map_easy_content).unwrap();

        let map_insane_content = "osu file format v14\n\n[Metadata]\nTitle:SetSong\nArtist:SetArtist\nCreator:Mapper\nVersion:Insane\nBeatmapID:7002\nBeatmapSetID:9000\n[Difficulty]\nHPDrainRate:7\nCircleSize:4\nOverallDifficulty:8\nApproachRate:9\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n300,200,2000,1,0,0:0:0:0:\n";
        let mut bmap_insane = utils::parse_osu_file_to_beatmap(map_insane_content, Some(7002), Some(9000)).unwrap();
        bmap_insane.file_content = Some(map_insane_content.to_string());
        db::insert_beatmap(&conn, &bmap_insane).unwrap();
        db::update_beatmap_file_content(&conn, &bmap_insane.file_md5, map_insane_content).unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        app_state.player = Some(crate::types::player::Player::new("PlayerSetSwitch".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        // 1. NP on Normal difficulty
        let np1 = "\x01ACTION is listening to [https://osu.ppy.sh/s/9000 SetArtist - SetSong [Normal]]\x01";
        handle_chat_message(shared_state.clone(), "PlayerSetSwitch", np1, "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("Normal"), "Must calculate Normal diff");
            assert_eq!(s.last_np_map.as_ref().unwrap().version, "Normal");
        }

        // 2. Switch to Insane difficulty of the same mapset via /s/ link
        let np2 = "\x01ACTION is listening to [https://osu.ppy.sh/s/9000 SetArtist - SetSong [Insane]]\x01";
        handle_chat_message(shared_state.clone(), "PlayerSetSwitch", np2, "Tillerino").await;
        {
            let mut s = shared_state.write().await;
            let p = s.player.as_mut().unwrap();
            let q = p.clear_queue();
            let pkts = packets::split_packets(&q);
            assert!(!pkts.is_empty());
            let mut r = packets::PacketReader::new(pkts[0].payload);
            let _sender = r.read_string().unwrap();
            let msg = r.read_string().unwrap();
            assert!(msg.contains("Insane"), "Must calculate Insane diff, not stale Normal diff! Got: {}", msg);
            assert_eq!(s.last_np_map.as_ref().unwrap().version, "Insane");
        }
    }

    #[tokio::test]
    async fn test_chat_message_clearscores() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "ClearUser").unwrap();

        let mut bmap = Beatmap::blank();
        bmap.beatmap_id = 9991;
        bmap.file_md5 = "clear_user_md5".to_string();
        bmap.artist = "ClearArtist".to_string();
        bmap.title = "ClearSong".to_string();
        bmap.version = "Expert".to_string();
        bmap.approved = 1;
        db::insert_beatmap(&conn, &bmap).unwrap();

        let sc = crate::types::score::Score {
            mode: 0,
            md5: "clear_user_md5".to_string(),
            name: "ClearUser".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 500000,
            max_combo: 300,
            perfect: true,
            mods: 0,
            time: 12345,
            acc: Some(100.0),
            pp: Some(150.0),
            replay_md5: None,
            scoreid: None,
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };
        db::insert_score(&conn, &sc, "ranked").unwrap();

        let config = crate::types::config::Config::default();
        let mut app_state = AppState::new(conn, config);
        let mut player = crate::types::player::Player::new("ClearUser".to_string());
        player.map_md5 = "clear_user_md5".to_string();
        app_state.player = Some(player);
        let state = Arc::new(RwLock::new(app_state));

        // 1. In #osu channel: !clearscores
        handle_chat_message(state.clone(), "ClearUser", "!clearscores", "#osu").await;

        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores = db::get_scores_on_map(&db_conn, "ClearUser", "clear_user_md5", 0).unwrap();
        assert_eq!(scores.len(), 0, "Score must be cleared after !clearscores in channel");
        drop(db_conn);
        drop(s);

        // 2. Re-insert score, then test in Tillerino PM
        {
            let s = state.read().await;
            let db_conn = s.db.lock().await;
            db::insert_score(&db_conn, &sc, "ranked").unwrap();
        }

        handle_chat_message(state.clone(), "ClearUser", "!clearscores", "Tillerino").await;

        let s = state.read().await;
        let db_conn = s.db.lock().await;
        let scores_after = db::get_scores_on_map(&db_conn, "ClearUser", "clear_user_md5", 0).unwrap();
        assert_eq!(scores_after.len(), 0, "Score must be cleared after !clearscores in Tillerino PM");
    }
}
