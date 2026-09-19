use crate::db;
use crate::server::response::Response;
use crate::state::AppState;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

const TILLERINO_AVATAR: &[u8] = include_bytes!("../../assets/tillerino.png");
const BANCHOBOT_AVATAR: &[u8] = include_bytes!("../../assets/banchobot.png");

pub async fn handle(state: Arc<RwLock<AppState>>, userid: i32) -> Response {
    if userid == 4 || userid == 2070907 {
        return Response::image(TILLERINO_AVATAR.to_vec(), "png");
    }

    if userid == 3 {
        return Response::image(BANCHOBOT_AVATAR.to_vec(), "png");
    }

    let s = state.read().await;

    if userid != 2 {
        let avatar_cache_dir = std::path::Path::new(".data").join("avatars");
        let cached_file = avatar_cache_dir.join(format!("{}.png", userid));
        if cached_file.exists() {
            if let Ok(bytes) = std::fs::read(&cached_file) {
                return Response::image(bytes, "png");
            }
        }

        let url = format!("https://a.ppy.sh/{}?.png", userid);
        match s.http.get(&url).send().await {
            Ok(resp) if resp.status() == 200 => {
                if let Ok(bytes) = resp.bytes().await {
                    let _ = std::fs::create_dir_all(&avatar_cache_dir);
                    let _ = std::fs::write(&cached_file, &bytes);
                    return Response::image(bytes.to_vec(), "png");
                }
            }
            _ => {}
        }
        return Response::image(s.default_avatar.clone(), "png");
    }

    let player_name = s
        .player
        .as_ref()
        .map(|p| p.name.clone())
        .or_else(|| s.pending_login_name.clone())
        .or_else(|| s.config.osu_username.clone());

    let avatar_url = if let Some(ref name) = player_name {
        let db_conn = s.db.lock().await;
        db::get_avatar(&db_conn, name).unwrap_or(None)
    } else {
        None
    };

    let Some(pfp) = avatar_url else {
        return Response::image(s.default_avatar.clone(), "png");
    };

    if let Some(path) = utils::is_path(&pfp) {
        match std::fs::read(&path) {
            Ok(bytes) => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("png");
                return Response::image(bytes, ext);
            }
            Err(_) => return Response::image(s.default_avatar.clone(), "png"),
        }
    }

    match s.http.get(&pfp).send().await {
        Ok(resp) if resp.status() == 200 => {
            if let Ok(bytes) = resp.bytes().await {
                return Response::image(bytes.to_vec(), "png");
            }
        }
        _ => {}
    }

    Response::image(s.default_avatar.clone(), "png")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::config::Config;
    use crate::types::player::Player;

    #[tokio::test]
    async fn test_bot_avatars_and_tillerino_pfp() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut app_state = AppState::new(conn, Config::default());
        app_state.player = Some(Player::new("PlayerTest".to_string()));
        let shared_state = Arc::new(RwLock::new(app_state));

        // Tillerino ID 4
        let resp_tillerino = handle(shared_state.clone(), 4).await;
        assert_eq!(resp_tillerino.status, hyper::StatusCode::OK);
        assert_eq!(resp_tillerino.body, TILLERINO_AVATAR);

        // Tillerino official user ID 2070907
        let resp_tillerino_official = handle(shared_state.clone(), 2070907).await;
        assert_eq!(resp_tillerino_official.status, hyper::StatusCode::OK);
        assert_eq!(resp_tillerino_official.body, TILLERINO_AVATAR);

        // BanchoBot ID 3
        let resp_bancho = handle(shared_state.clone(), 3).await;
        assert_eq!(resp_bancho.status, hyper::StatusCode::OK);
        assert_eq!(resp_bancho.body, BANCHOBOT_AVATAR);
    }

    #[tokio::test]
    async fn test_avatar_pre_login_safety() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        let mut app_state = AppState::new(conn, Config::default());
        app_state.player = None; // Pre-login state
        app_state.default_avatar = vec![1, 2, 3, 4];
        let shared_state = Arc::new(RwLock::new(app_state));

        // Requesting local player (ID 2) when player is not yet set must not panic
        let resp = handle(shared_state, 2).await;
        assert_eq!(resp.status, hyper::StatusCode::OK);
        assert_eq!(resp.body, vec![1, 2, 3, 4]);
    }
}
