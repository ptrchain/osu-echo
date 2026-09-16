use crate::db;
use crate::server::response::Response;
use crate::state::AppState;
use crate::utils;
use std::sync::Arc;
use tokio::sync::RwLock;

pub async fn handle(state: Arc<RwLock<AppState>>, userid: i32) -> Response {
    let s = state.read().await;

    if s.player.is_none() {
        return Response::empty().with_status(hyper::StatusCode::NOT_FOUND);
    }

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
