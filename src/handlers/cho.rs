use crate::db;
use crate::packets;
use crate::server::response::Response;
use crate::state::AppState;
use crate::types::player::Player;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

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

pub async fn handle(state: Arc<RwLock<AppState>>, osu_token: Option<&str>) -> Response {
    match osu_token {
        None | Some("") => {
            let (body, token) = login(state).await;
            Response::new(body).with_header("cho-token", &token)
        }
        Some(_) => {
            let mut s = state.write().await;
            if s.player.is_none() {
                return Response::new(packets::system_restart(0));
            }

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

async fn login(state: Arc<RwLock<AppState>>) -> (Vec<u8>, String) {
    let mut body = Vec::new();

    // Bancho hits bancho_connect.php right before /c/ login POST; poll briefly to synchronize username
    let mut name: Option<String> = None;
    for _ in 0..5 {
        {
            let s = state.read().await;
            if s.pending_login_name.is_some() {
                name = s.pending_login_name.clone();
                break;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
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

    {
        let s = state.read().await;
        let db = s.db.lock().await;
        let _ = db::ensure_profile(&db, &profile_name);
        let _ = db::ensure_avatar(&db, &profile_name);
    }

    let mut player = Player::new(profile_name.clone());

    body.extend_from_slice(&packets::user_id(player.userid));
    body.extend_from_slice(&packets::notification("Welcome to the local osu! server (Rust edition)!\nSave replays to submit scores."));
    body.extend_from_slice(&packets::protocol_version(19));
    body.extend_from_slice(&packets::bancho_privs(player.bancho_privs));
    body.extend_from_slice(&packets::friends_list(&[0]));

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

    body.extend_from_slice(&packets::user_presence(&player));
    body.extend_from_slice(&packets::user_stats(&player));

    body.extend_from_slice(&packets::local_message("Welcome! Type !help in the direct search bar to see commands.", "#osu"));

    utils::log_success(&format!("{} successfully logged in!", profile_name));

    {
        let mut s = state.write().await;
        s.player = Some(player);
    }

    (body, "success".to_string())
}
