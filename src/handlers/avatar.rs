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
        let url = format!("https://a.ppy.sh/{}?.png", userid);
        match s.http.get(&url).send().await {
            Ok(resp) if resp.status() == 200 => {
                if let Ok(bytes) = resp.bytes().await {
                    return Response::image(bytes.to_vec(), "png");
                }
            }
            _ => {}
        }
        return Response::image(s.default_avatar.clone(), "png");
    }

    let player_name = s.player.as_ref().unwrap().name.clone();
    let db_conn = s.db.lock().await;
    let avatar_url: Option<String> = db::get_avatar(&db_conn, &player_name).unwrap_or(None);
    drop(db_conn);

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
}
