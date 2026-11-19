use crate::db;
use crate::handlers::banchobot;
use crate::handlers::cho::safe_logout_bots;
use crate::multiplayer::bot::{sanitize_freemod_bits, BotProfile, BotSource, SkillProfile};
use crate::multiplayer::match_state::Team;
use crate::packets;
use crate::state::AppState;
use crate::types::mods::Mods;
use std::sync::Arc;
use tokio::sync::RwLock;

const MAX_BOT_NAME_LEN: usize = 32;

pub fn sanitize_bot_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_BOT_NAME_LEN)
        .collect();
    if cleaned.is_empty() {
        "Bot".to_string()
    } else {
        cleaned
    }
}

pub fn clamp_sr(sr: f64) -> f64 {
    if !sr.is_finite() {
        return 5.0;
    }
    sr.clamp(0.5, 15.0)
}

pub fn clamp_acc(acc: f64) -> f64 {
    if !acc.is_finite() {
        return 97.0;
    }
    acc.clamp(50.0, 100.0)
}

pub fn clamp_unit(v: f64) -> f64 {
    if !v.is_finite() {
        return 0.5;
    }
    v.clamp(0.0, 1.0)
}

fn parse_slot_1based(raw: &str) -> Result<usize, &'static str> {
    let n: usize = raw.parse().map_err(|_| "Slot must be a number between 1 and 16.")?;
    if !(1..=16).contains(&n) {
        return Err("Slot must be between 1 and 16.");
    }
    Ok(n - 1)
}

fn parse_freemod_arg(raw: &str) -> u32 {
    let trimmed = raw.trim().trim_start_matches('+');
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("NM") || trimmed.eq_ignore_ascii_case("NONE") {
        return 0;
    }
    sanitize_freemod_bits(Mods::from_short_str(trimmed).bits())
}

pub async fn handle_mp_command(state: Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    if args.is_empty() || args[0].eq_ignore_ascii_case("help") {
        let help = "MP Bot Commands: !mp addbot <6digit..1digit|rival|custom> [name] [sr=X] [acc=Y] [mods=+HD] [nofail] | !mp removebot <slot|name> | !mp clearbots | !mp botskill <slot> <sr> [acc] | !mp botmods <slot> <+HDHR|NM> | !mp team <slot> <red|blue> | !mp seed <u64> | !mp abort";
        banchobot::reply(&state, target, help).await;
        return;
    }

    let sub = args[0].to_ascii_lowercase();
    let sub_args = &args[1..];

    let has_match = {
        let s = state.read().await;
        s.active_match.is_some()
    };
    if !has_match {
        banchobot::reply(&state, target, "You are not currently in a multiplayer room.").await;
        return;
    }

    match sub.as_str() {
        "addbot" => handle_addbot(state, player_name, target, sub_args).await,
        "removebot" => handle_removebot(state, player_name, target, sub_args).await,
        "clearbots" => handle_clearbots(state, player_name, target).await,
        "botskill" => handle_botskill(state, target, sub_args).await,
        "botmods" => handle_botmods(state, target, sub_args).await,
        "team" => handle_team(state, target, sub_args).await,
        "seed" => handle_seed(state, target, sub_args).await,
        "abort" => handle_abort(state, target).await,
        _ => {
            banchobot::reply(&state, target, &format!("Unknown !mp subcommand '{}'. Type !mp help for usage.", sub)).await;
        }
    }
}

async fn handle_addbot(state: Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    if args.is_empty() {
        banchobot::reply(
            &state,
            target,
            "Usage: !mp addbot <6digit..1digit|rival|custom|username> [name] [sr=X] [acc=Y] [mods=+HD] [nofail]",
        )
        .await;
        return;
    }

    let kind = args[0].to_ascii_lowercase();
    let is_builtin = matches!(kind.as_str(), "6digit" | "5digit" | "4digit" | "3digit" | "2digit" | "1digit" | "rival" | "custom");

    // Resolve real osu! player outside AppState locks when not a built-in preset
    let resolved_mimic = if !is_builtin {
        match crate::multiplayer::mimic::resolve_player(&state, player_name, args[0]).await {
            Ok(profile) => Some(profile),
            Err(err) => {
                banchobot::reply(&state, target, &err).await;
                return;
            }
        }
    } else {
        None
    };

    let mut s = state.write().await;

    let Some(ref player_snapshot) = s.player else {
        return;
    };
    let rival_template = BotProfile::rival_for(player_snapshot, 0);

    let Some(ref mut room) = s.active_match else {
        return;
    };

    let mut profile = if let Some(mimic_profile) = resolved_mimic {
        mimic_profile
    } else {
        let synth_id = room.alloc_synthetic_bot_id();
        match kind.as_str() {
            "6digit" | "5digit" | "4digit" | "3digit" | "2digit" | "1digit" => BotProfile::from_preset(&kind, synth_id).unwrap(),
            "rival" => {
                let mut r = rival_template;
                r.userid = synth_id;
                r
            }
            _ => BotProfile {
                userid: synth_id,
                name: "Custom Bot".to_string(),
                country: 0,
                rank: 50_000,
                pp: 4_000,
                acc: 97.0,
                source: BotSource::Procedural(SkillProfile::default()),
            },
        }
    };

    let is_mimic = matches!(profile.source, BotSource::MimicPlayer { .. });
    let mut custom_name_set = is_mimic;
    for &tok in &args[1..] {
        if let Some((k, v)) = tok.split_once('=') {
            match k.to_ascii_lowercase().as_str() {
                "sr" => {
                    if let Ok(sr) = v.parse::<f64>() {
                        if let BotSource::Procedural(ref mut sk) | BotSource::MimicPlayer { fallback_skill: ref mut sk, .. } = profile.source {
                            sk.target_sr = clamp_sr(sr);
                        }
                    }
                }
                "acc" => {
                    if let Ok(acc) = v.parse::<f64>() {
                        let clamped = clamp_acc(acc);
                        profile.acc = clamped;
                        if let BotSource::Procedural(ref mut sk) | BotSource::MimicPlayer { fallback_skill: ref mut sk, .. } = profile.source {
                            sk.accuracy_mean = clamped;
                        }
                    }
                }
                "consistency" | "cons" => {
                    if let Ok(c) = v.parse::<f64>() {
                        if let BotSource::Procedural(ref mut sk) | BotSource::MimicPlayer { fallback_skill: ref mut sk, .. } = profile.source {
                            sk.consistency = clamp_unit(c);
                        }
                    }
                }
                "choke" => {
                    if let Ok(ch) = v.parse::<f64>() {
                        if let BotSource::Procedural(ref mut sk) | BotSource::MimicPlayer { fallback_skill: ref mut sk, .. } = profile.source {
                            sk.choke_tendency = clamp_unit(ch);
                        }
                    }
                }
                "mods" => {
                    let m = parse_freemod_arg(v);
                    if let BotSource::Procedural(ref mut sk) | BotSource::MimicPlayer { fallback_skill: ref mut sk, .. } = profile.source {
                        sk.preferred_mods = m;
                    }
                }
                _ => {}
            }
        } else if tok.eq_ignore_ascii_case("nofail") || tok.eq_ignore_ascii_case("nf") {
            if let BotSource::Procedural(ref mut sk) | BotSource::MimicPlayer { fallback_skill: ref mut sk, .. } = profile.source {
                sk.can_fail = false;
            }
        } else if !custom_name_set {
            profile.name = sanitize_bot_name(tok);
            custom_name_set = true;
        }
    }

    let bp = profile.to_bancho_player(room.mode);
    let summary_name = profile.name.clone();
    let (sr_str, acc_str) = match &profile.source {
        BotSource::Procedural(sk) | BotSource::MimicPlayer { fallback_skill: sk, .. } => {
            (format!("{:.1}*", sk.target_sr), format!("{:.1}%", sk.accuracy_mean))
        }
        _ => ("?".to_string(), format!("{:.1}%", profile.acc)),
    };
    let beatmap_id = room.beatmap_id as i64;
    let mode = room.mode;

    match room.add_bot(profile) {
        Ok(slot_idx) => {
            let wire = room.to_wire();
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::user_presence(&bp));
                p.queue.extend_from_slice(&packets::user_stats(&bp));
                p.queue.extend_from_slice(&packets::update_match(&wire, true));
            }
            drop(s);
            if is_mimic && beatmap_id > 0 {
                crate::multiplayer::mimic::spawn_prefetch_mimic_scores(state.clone(), beatmap_id, mode);
            }
            let tag = if is_mimic { " [Mimic]" } else { "" };
            banchobot::reply(
                &state,
                target,
                &format!("Added {}{} to slot {} ({}, {} acc).", summary_name, tag, slot_idx + 1, sr_str, acc_str),
            )
            .await;
        }
        Err(msg) => {
            drop(s);
            banchobot::reply(&state, target, msg).await;
        }
    }
}

async fn handle_removebot(state: Arc<RwLock<AppState>>, player_name: &str, target: &str, args: &[&str]) {
    if args.is_empty() {
        banchobot::reply(&state, target, "Usage: !mp removebot <slot|name>").await;
        return;
    }

    let friend_ids = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
    };

    let query = args.join(" ");
    let mut s = state.write().await;

    let result: Result<(usize, BotProfile, packets::MatchWire), String> = if let Some(ref mut room) = s.active_match {
        if let Ok(idx) = parse_slot_1based(&query) {
            if room.slots[idx].is_human() {
                Err("Cannot remove the human player slot.".to_string())
            } else if let Some(removed) = room.remove_bot_at(idx) {
                Ok((idx, removed, room.to_wire()))
            } else {
                Err(format!("Slot {} does not contain a bot.", idx + 1))
            }
        } else if query.chars().all(|c| c.is_ascii_digit() || c == '-') && !query.is_empty() {
            Err("Slot must be between 1 and 16.".to_string())
        } else {
            let found = room.slots.iter().position(|sl| {
                sl.bot.as_ref().is_some_and(|b| b.profile.name.eq_ignore_ascii_case(&query))
            });
            if let Some(idx) = found {
                let removed = room.remove_bot_at(idx).unwrap();
                Ok((idx, removed, room.to_wire()))
            } else {
                Err(format!("No bot found matching '{}'.", query))
            }
        }
    } else {
        Err("You are not currently in a multiplayer room.".to_string())
    };

    match result {
        Ok((idx, removed, wire)) => {
            if let Some(ref mut p) = s.player {
                safe_logout_bots(p, &[removed.userid], &friend_ids);
                p.queue.extend_from_slice(&packets::update_match(&wire, true));
            }
            drop(s);
            banchobot::reply(&state, target, &format!("Removed {} from slot {}.", removed.name, idx + 1)).await;
        }
        Err(err) => {
            drop(s);
            banchobot::reply(&state, target, &err).await;
        }
    }
}

async fn handle_clearbots(state: Arc<RwLock<AppState>>, player_name: &str, target: &str) {
    let friend_ids = {
        let s = state.read().await;
        let db_conn = s.db.lock().await;
        db::get_friend_ids(&db_conn, player_name).unwrap_or_default()
    };

    let mut s = state.write().await;
    let outcome = s.active_match.as_mut().map(|room| {
        let removed = room.clear_bots();
        let ids: Vec<i32> = removed.iter().map(|b| b.userid).collect();
        (ids, room.to_wire())
    });

    if let Some((removed_ids, wire)) = outcome {
        let count = removed_ids.len();
        if let Some(ref mut p) = s.player {
            safe_logout_bots(p, &removed_ids, &friend_ids);
            p.queue.extend_from_slice(&packets::update_match(&wire, true));
        }
        drop(s);
        banchobot::reply(&state, target, &format!("Cleared {} bot(s) from the room.", count)).await;
    }
}

async fn handle_botskill(state: Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    if args.len() < 2 {
        banchobot::reply(&state, target, "Usage: !mp botskill <slot> <sr> [acc]").await;
        return;
    }
    let idx = match parse_slot_1based(args[0]) {
        Ok(i) => i,
        Err(e) => {
            banchobot::reply(&state, target, e).await;
            return;
        }
    };
    let Ok(sr_raw) = args[1].parse::<f64>() else {
        banchobot::reply(&state, target, "Invalid star rating value.").await;
        return;
    };
    let sr = clamp_sr(sr_raw);
    let acc_opt = match args.get(2) {
        Some(a) => match a.parse::<f64>() {
            Ok(v) => Some(clamp_acc(v)),
            Err(_) => {
                banchobot::reply(&state, target, "Invalid accuracy value.").await;
                return;
            }
        },
        None => None,
    };

    let mut s = state.write().await;
    let res = if let Some(ref mut room) = s.active_match {
        if room.slots[idx].is_human() {
            Err("Cannot modify the human player slot.".to_string())
        } else if let Some(ref mut bot) = room.slots[idx].bot {
            let mode = room.mode;
            let updated = match &mut bot.profile.source {
                BotSource::Procedural(sk) | BotSource::MimicPlayer { fallback_skill: sk, .. } => {
                    sk.target_sr = sr;
                    if let Some(acc) = acc_opt {
                        sk.accuracy_mean = acc;
                    }
                    Some((sk.target_sr, sk.accuracy_mean))
                }
                _ => None,
            };
            if let Some((new_sr, new_acc)) = updated {
                if let Some(acc) = acc_opt {
                    bot.profile.acc = acc;
                }
                let bp = bot.profile.to_bancho_player(mode);
                Ok((bot.profile.name.clone(), new_sr, new_acc, bp))
            } else {
                Err("That bot is a ghost and does not use a procedural skill profile.".to_string())
            }
        } else {
            Err(format!("Slot {} does not contain a bot.", idx + 1))
        }
    } else {
        Err("You are not currently in a multiplayer room.".to_string())
    };

    match res {
        Ok((name, new_sr, new_acc, bp)) => {
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::user_stats(&bp));
            }
            drop(s);
            banchobot::reply(&state, target, &format!("Updated {} (slot {}): {:.1}*, {:.1}% acc.", name, idx + 1, new_sr, new_acc)).await;
        }
        Err(e) => {
            drop(s);
            banchobot::reply(&state, target, &e).await;
        }
    }
}

async fn handle_botmods(state: Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    if args.len() < 2 {
        banchobot::reply(&state, target, "Usage: !mp botmods <slot> <+HDHR|NM>").await;
        return;
    }
    let idx = match parse_slot_1based(args[0]) {
        Ok(i) => i,
        Err(e) => {
            banchobot::reply(&state, target, e).await;
            return;
        }
    };
    let mods_bits = parse_freemod_arg(args[1]);
    let mods_label = Mods::from_bits_truncate(mods_bits).short_name();

    let mut s = state.write().await;
    let res = if let Some(ref mut room) = s.active_match {
        if room.slots[idx].is_human() {
            Err("Cannot modify the human player slot.".to_string())
        } else if let Some(ref mut bot) = room.slots[idx].bot {
            match &mut bot.profile.source {
                BotSource::Procedural(sk) | BotSource::MimicPlayer { fallback_skill: sk, .. } => {
                    sk.preferred_mods = mods_bits;
                }
                _ => {}
            }
            let name = bot.profile.name.clone();
            if room.freemods {
                room.slots[idx].mods = mods_bits;
            }
            Ok((name, room.to_wire()))
        } else {
            Err(format!("Slot {} does not contain a bot.", idx + 1))
        }
    } else {
        Err("You are not currently in a multiplayer room.".to_string())
    };

    match res {
        Ok((name, wire)) => {
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::update_match(&wire, true));
            }
            drop(s);
            banchobot::reply(&state, target, &format!("Set {} (slot {}) mods to +{}.", name, idx + 1, mods_label)).await;
        }
        Err(e) => {
            drop(s);
            banchobot::reply(&state, target, &e).await;
        }
    }
}

async fn handle_team(state: Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    if args.len() < 2 {
        banchobot::reply(&state, target, "Usage: !mp team <slot> <red|blue>").await;
        return;
    }
    let idx = match parse_slot_1based(args[0]) {
        Ok(i) => i,
        Err(e) => {
            banchobot::reply(&state, target, e).await;
            return;
        }
    };
    let team = match args[1].to_ascii_lowercase().as_str() {
        "red" | "r" => Team::Red,
        "blue" | "b" => Team::Blue,
        _ => {
            banchobot::reply(&state, target, "Team must be 'red' or 'blue'.").await;
            return;
        }
    };

    let mut s = state.write().await;
    let res = if let Some(ref mut room) = s.active_match {
        if room.slots[idx].is_human() {
            Err("Use the in-game team toggle to change your own team.".to_string())
        } else if let Some(ref bot) = room.slots[idx].bot {
            let name = bot.profile.name.clone();
            room.slots[idx].team = team;
            Ok((name, room.to_wire()))
        } else {
            Err(format!("Slot {} does not contain a bot.", idx + 1))
        }
    } else {
        Err("You are not currently in a multiplayer room.".to_string())
    };

    match res {
        Ok((name, wire)) => {
            if let Some(ref mut p) = s.player {
                p.queue.extend_from_slice(&packets::update_match(&wire, true));
            }
            drop(s);
            let label = if team == Team::Red { "Red" } else { "Blue" };
            banchobot::reply(&state, target, &format!("Assigned {} (slot {}) to {} team.", name, idx + 1, label)).await;
        }
        Err(e) => {
            drop(s);
            banchobot::reply(&state, target, &e).await;
        }
    }
}

async fn handle_seed(state: Arc<RwLock<AppState>>, target: &str, args: &[&str]) {
    let Some(raw) = args.first() else {
        banchobot::reply(&state, target, "Usage: !mp seed <u64>").await;
        return;
    };
    let Ok(seed) = raw.parse::<u64>() else {
        banchobot::reply(&state, target, "Seed must be a positive integer (u64).").await;
        return;
    };

    let mut s = state.write().await;
    if let Some(ref mut room) = s.active_match {
        room.rng_seed_override = Some(seed);
        room.seed = (seed & 0x7FFF_FFFF) as i32;
    }
    drop(s);
    banchobot::reply(&state, target, &format!("Match RNG seed set to {}.", seed)).await;
}

async fn handle_abort(state: Arc<RwLock<AppState>>, target: &str) {
    let mut s = state.write().await;
    let wire_opt = if let Some(ref mut room) = s.active_match {
        if !room.in_progress {
            None
        } else {
            room.reset_after_match();
            Some(room.to_wire())
        }
    } else {
        None
    };

    if let Some(wire) = wire_opt {
        if let Some(ref mut p) = s.player {
            p.queue.extend_from_slice(&packets::match_abort());
            p.queue.extend_from_slice(&packets::update_match(&wire, true));
        }
        drop(s);
        banchobot::reply(&state, target, "Match aborted.").await;
    } else {
        drop(s);
        banchobot::reply(&state, target, "No match is currently in progress.").await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiplayer::match_state::{MatchRoom, SlotStatus};
    use crate::types::config::Config;
    use crate::types::player::Player;

    async fn setup_room_state(freemods: bool) -> Arc<RwLock<AppState>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "Host").unwrap();
        let mut state = AppState::new(conn, Config::default());
        let mut p = Player::new("Host".to_string());
        p.pp = 4_500;
        p.acc = 98.0;
        state.player = Some(p);

        let mut wire = packets::MatchWire::default();
        wire.name = "Cmd Room".to_string();
        wire.freemods = u8::from(freemods);
        wire.slot_status = [SlotStatus::Open as u8; 16];
        state.active_match = Some(MatchRoom::from_create_wire(1, 2, &wire));
        Arc::new(RwLock::new(state))
    }

    #[tokio::test]
    async fn test_addbot_clamping_sanitization_and_packets() {
        let state = setup_room_state(true).await;

        handle_mp_command(
            state.clone(),
            "Host",
            "#multiplayer",
            &[
                "addbot",
                "custom",
                "SuperLongBotNameWithWayTooManyCharacters1234567890",
                "sr=99.0",
                "acc=12.0",
                "consistency=5.0",
                "choke=-2.0",
                "mods=+HDDTHR",
                "nofail",
            ],
        )
        .await;

        let s = state.read().await;
        let room = s.active_match.as_ref().unwrap();
        let bot = room.slots[1].bot.as_ref().expect("bot in slot 1");
        assert_eq!(bot.profile.name.len(), MAX_BOT_NAME_LEN);
        let BotSource::Procedural(ref sk) = bot.profile.source else { panic!() };
        assert!((sk.target_sr - 15.0).abs() < 1e-6);
        assert!((sk.accuracy_mean - 50.0).abs() < 1e-6);
        assert!((sk.consistency - 1.0).abs() < 1e-6);
        assert!((sk.choke_tendency - 0.0).abs() < 1e-6);
        assert!(!sk.can_fail);
        // DT stripped from FreeMod slot, HD+HR preserved
        assert_eq!(room.slots[1].mods, (Mods::HIDDEN | Mods::HARDROCK).bits());

        let pkts = packets::split_packets(&s.player.as_ref().unwrap().queue);
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoUserPresence as u16));
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoUserStats as u16));
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoUpdateMatch as u16));
    }

    #[tokio::test]
    async fn test_slot_validation_human_protection_and_commands() {
        let state = setup_room_state(true).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["addbot", "5digit", "Alpha"]).await;

        // Human slot (1) is protected from removebot, botskill, botmods, team
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["removebot", "1"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["botskill", "1", "8.0"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["botmods", "1", "+HD"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["team", "1", "red"]).await;
        // Out of range slots are rejected cleanly
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["removebot", "0"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["removebot", "17"]).await;

        {
            let s = state.read().await;
            let room = s.active_match.as_ref().unwrap();
            assert!(room.slots[0].is_human());
            assert!(room.slots[1].is_bot());
        }

        // Valid mutations on slot 2 (1-based)
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["botskill", "2", "7.5", "99.1"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["botmods", "2", "+HR"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["team", "2", "red"]).await;
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["seed", "123456789"]).await;

        {
            let s = state.read().await;
            let room = s.active_match.as_ref().unwrap();
            let BotSource::Procedural(ref sk) = room.slots[1].bot.as_ref().unwrap().profile.source else { panic!() };
            assert!((sk.target_sr - 7.5).abs() < 1e-6);
            assert!((sk.accuracy_mean - 99.1).abs() < 1e-6);
            assert_eq!(room.slots[1].mods, Mods::HARDROCK.bits());
            assert_eq!(room.slots[1].team, Team::Red);
            assert_eq!(room.rng_seed_override, Some(123456789));
        }

        // Remove by name
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["removebot", "alpha"]).await;
        assert!(state.read().await.active_match.as_ref().unwrap().slots[1].is_open());
    }

    #[tokio::test]
    async fn test_room_full_and_abort() {
        let state = setup_room_state(false).await;
        // Fill remaining 15 slots
        for _ in 0..15 {
            handle_mp_command(state.clone(), "Host", "#multiplayer", &["addbot", "5digit"]).await;
        }
        // 16th bot should fail gracefully without panicking
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["addbot", "5digit"]).await;
        assert!(state.read().await.active_match.as_ref().unwrap().find_open_slot().is_none());

        // Abort in-progress match
        {
            let mut s = state.write().await;
            let room = s.active_match.as_mut().unwrap();
            room.in_progress = true;
            room.slots[0].status = SlotStatus::Playing;
            s.player.as_mut().unwrap().clear_queue();
        }
        handle_mp_command(state.clone(), "Host", "#multiplayer", &["abort"]).await;
        let s = state.read().await;
        let room = s.active_match.as_ref().unwrap();
        assert!(!room.in_progress);
        assert_eq!(room.slots[0].status, SlotStatus::NotReady);
        let pkts = packets::split_packets(&s.player.as_ref().unwrap().queue);
        assert!(pkts.iter().any(|p| p.id == packets::PacketId::ChoMatchAbort as u16));
    }
}
