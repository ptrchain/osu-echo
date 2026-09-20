use crate::db;
use crate::packets;
use crate::state::AppState;
use crate::types::command::{Command, CommandHandler};
use crate::types::direct_response::DirectResponse;
use std::sync::Arc;
use tokio::sync::RwLock;

pub fn register_commands(state: &mut AppState) {
    let commands: Vec<(Vec<&str>, Option<&str>, bool, CommandHandler)> = vec![
        (vec!["help", "h"], Some("shows all commands available!"), false, Arc::new(|state, _args| Box::pin(cmd_help(state)))),
        (vec!["tops", "t"], Some("shows your top plays!"), false, Arc::new(|state, args| Box::pin(cmd_tops(state, args)))),
        (vec!["r", "rs", "recent"], Some("shows your recent plays!"), false, Arc::new(|state, args| Box::pin(cmd_recent(state, args)))),
        (vec!["stats", "p", "osu", "profile", "s"], Some("shows your stats!"), false, Arc::new(|state, args| Box::pin(cmd_stats(state, args)))),
        (vec!["recalc", "recalculate"], Some("recalculate all profiles!"), true, Arc::new(|state, _args| Box::pin(cmd_recalc(state)))),
        (vec!["wipe"], Some("wipes all stats from your current profile!"), true, Arc::new(|state, _args| Box::pin(cmd_wipe(state)))),
        (vec!["avatar"], Some("change your avatar! Example: !avatar (path or URL)"), true, Arc::new(|state, args| Box::pin(cmd_avatar(state, args)))),
        (vec!["config"], Some("shows current config!"), false, Arc::new(|state, _args| Box::pin(cmd_config(state)))),
    ];

    for (names, docs, confirm, func) in commands {
        let primary = names[0].to_string();
        let all_names: Vec<String> = names.iter().map(|s| s.to_string()).collect();

        let cmd = Command::new(primary.clone(), all_names.clone(), docs.map(String::from), confirm, func);

        for name in &all_names {
            state.commands.insert(name.clone(), cmd.clone());
        }
    }
}

async fn cmd_help(state: Arc<RwLock<AppState>>) -> Option<Vec<u8>> {
    let s = state.read().await;
    let mut msg = Vec::new();
    let mut existing_docs = Vec::new();

    for cmd in s.commands.values() {
        if let Some(ref docs) = cmd.docs {
            if existing_docs.contains(docs) {
                continue;
            }
            existing_docs.push(docs.clone());
        }

        let alias = cmd.names.iter().map(|n| format!("{}{}", s.config.command_prefix, n)).collect::<Vec<_>>().join(" / ");

        if let Some(ref docs) = cmd.docs {
            msg.push(format!("{} // {}", alias, docs));
        } else {
            msg.push(alias);
        }
    }

    let resp = DirectResponse::from_str(&msg.join("\n"), -2);
    Some(resp.as_binary())
}

async fn cmd_tops(state: Arc<RwLock<AppState>>, args: Vec<String>) -> Option<Vec<u8>> {
    let mode = args.first().map(|s| s.as_str());
    let limit: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);

    let s = state.read().await;
    let player = s.player.as_ref()?;
    let player_name = player.name.clone();
    let default_mode = player.mode.to_string();

    let mut params = std::collections::HashMap::new();
    params.insert("u".to_string(), player_name);
    params.insert("limit".to_string(), limit.to_string());
    params.insert("m".to_string(), mode.unwrap_or(&default_mode).to_string());

    let resp = crate::handlers::api::handle(state.clone(), "/tops", &params).await;
    Some(resp.body)
}

async fn cmd_recent(state: Arc<RwLock<AppState>>, args: Vec<String>) -> Option<Vec<u8>> {
    let mode = args.first().map(|s| s.as_str());
    let limit: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);

    let s = state.read().await;
    let player = s.player.as_ref()?;
    let player_name = player.name.clone();
    let default_mode = player.mode.to_string();

    let mut params = std::collections::HashMap::new();
    params.insert("u".to_string(), player_name);
    params.insert("limit".to_string(), limit.to_string());
    params.insert("m".to_string(), mode.unwrap_or(&default_mode).to_string());

    let resp = crate::handlers::api::handle(state.clone(), "/recent", &params).await;
    Some(resp.body)
}

async fn cmd_stats(state: Arc<RwLock<AppState>>, args: Vec<String>) -> Option<Vec<u8>> {
    let mode = args.first().map(|s| s.as_str());

    let s = state.read().await;
    let player = s.player.as_ref()?;
    let player_name = player.name.clone();
    let default_mode = player.mode.to_string();

    let mut params = std::collections::HashMap::new();
    params.insert("u".to_string(), player_name);
    params.insert("m".to_string(), mode.unwrap_or(&default_mode).to_string());

    let resp = crate::handlers::api::handle(state.clone(), "/profile", &params).await;

    if let Ok(json) = serde_json::from_slice::<serde_json::Value>(&resp.body) {
        let msg = format!(
            "name: {}\nrank: #{}\nplaycount: {}\npp: {}\nacc: {:.2}%\nmode: {}",
            json.get("name").and_then(|v| v.as_str()).unwrap_or("?"),
            json.get("rank").and_then(|v| v.as_i64()).unwrap_or(0),
            json.get("playcount").and_then(|v| v.as_i64()).unwrap_or(0),
            json.get("pp").and_then(|v| v.as_i64()).unwrap_or(0),
            json.get("acc").and_then(|v| v.as_f64()).unwrap_or(0.0),
            json.get("mode").and_then(|v| v.as_str()).unwrap_or("vn"),
        );
        let dr = DirectResponse::from_str(&msg, -2);
        Some(dr.as_binary())
    } else {
        let dr = DirectResponse::from_str("Failed to get stats", -2);
        Some(dr.as_binary())
    }
}

async fn cmd_recalc(state: Arc<RwLock<AppState>>) -> Option<Vec<u8>> {
    let s = state.read().await;
    let player_name = s.player.as_ref()?.name.clone();
    drop(s);

    let mut params = std::collections::HashMap::new();
    params.insert("u".to_string(), player_name);

    crate::handlers::api::handle(state.clone(), "/recalc", &params).await;

    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        p.queue.extend_from_slice(&packets::notification("Stats recalculation started."));
    }
    None
}

async fn cmd_wipe(state: Arc<RwLock<AppState>>) -> Option<Vec<u8>> {
    let s = state.read().await;
    let player_name = s.player.as_ref()?.name.clone();
    drop(s);

    let mut params = std::collections::HashMap::new();
    params.insert("u".to_string(), player_name);
    params.insert("yes".to_string(), "yes".to_string());

    crate::handlers::api::handle(state.clone(), "/wipe", &params).await;

    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        p.queue.extend_from_slice(&packets::notification("Profile wiped."));
    }
    None
}

async fn cmd_avatar(state: Arc<RwLock<AppState>>, args: Vec<String>) -> Option<Vec<u8>> {
    let img = args.first()?.clone();

    let s = state.read().await;
    let player_name = s.player.as_ref()?.name.clone();

    let db_conn = s.db.lock().await;
    let _ = db::set_avatar(&db_conn, &player_name, &img);
    drop(db_conn);
    drop(s);

    let mut s = state.write().await;
    if let Some(ref mut p) = s.player {
        p.queue.extend_from_slice(&packets::notification("Avatar updated. Restart osu! to apply."));
    }
    None
}

async fn cmd_config(state: Arc<RwLock<AppState>>) -> Option<Vec<u8>> {
    let s = state.read().await;
    let config_str = format!("{:#?}", s.config);
    let resp = DirectResponse::from_str(&config_str, -2);
    Some(resp.as_binary())
}
