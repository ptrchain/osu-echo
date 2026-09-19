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

pub async fn handle_chat_message(state: Arc<RwLock<AppState>>, player_name: &str, message: &str, target: &str) {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return;
    }

    let is_tillerino = target.eq_ignore_ascii_case(tillerino::BOT_NAME);
    let is_banchobot = target.eq_ignore_ascii_case(banchobot::BOT_NAME);
    let is_pm = is_tillerino || is_banchobot || !target.starts_with('#');
    let reply_target = if is_pm { player_name.to_string() } else { target.to_string() };

    if !is_banchobot {
        if let Some(np_info) = parse_np_message(message) {
            tillerino::handle_np(&state, player_name, &reply_target, np_info).await;
            return;
        }
    }

    if trimmed.starts_with('\x01') {
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

    if is_tillerino {
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

    #[test]
    fn test_parse_np_message_direct() {
        assert!(parse_np_message("/np").is_some());
        assert!(parse_np_message("!np").is_some());
        assert!(parse_np_message("/np ").is_some());
        assert!(parse_np_message("hello").is_none());
    }

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

    #[test]
    fn test_parse_np_message_editing_and_slash_format() {
        let edit_msg = "\x01ACTION is editing [https://osu.ppy.sh/b/54321 MapArtist - EditTitle [Insane]] +HD\x01";
        let info = parse_np_message(edit_msg).unwrap();
        assert_eq!(info.map_id, Some(54321));
        assert_eq!(info.mods, Some(Mods::HIDDEN.bits()));

        let slash_msg = "\x01ACTION is listening to [https://osu.ppy.sh/beatmapsets/123456/789012 Artist - SlashTitle [Hard]]\x01";
        let info2 = parse_np_message(slash_msg).unwrap();
        assert_eq!(info2.set_id, Some(123456));
        assert_eq!(info2.map_id, Some(789012));
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
}
