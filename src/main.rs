#![allow(dead_code)]

mod commands;
mod db;
mod handlers;
mod logger;
mod packets;
mod server;
mod state;
mod types;
mod utils;

use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use server::response::Response;
use server::router::{match_route, RouteMatch};
use state::AppState;

pub const VERSION: &str = "1.0";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();

    logger::info(&format!("osu! Local Server (Rust) v{}", VERSION));

    let data_dir = std::env::current_dir()?.join(".data");
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("server.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    db::init_db(&conn)?;

    let mut config = match db::load_config(&conn)? {
        Some(config) => {
            logger::info("Found existing server configuration.");
            config
        }
        None => {
            let config = types::config::setup_config(&data_dir);
            db::save_config(&conn, &config)?;
            config
        }
    };

    config.apply_env_overrides();

    if let Some(ref replay_folder) = config.replay_folder() {
        std::fs::create_dir_all(replay_folder).ok();
    }

    let mut app_state = AppState::new(conn, config.clone());

    let http = &app_state.http;
    match http.get("https://a.ppy.sh/").send().await {
        Ok(resp) if resp.status() == 200 => {
            if let Ok(bytes) = resp.bytes().await {
                app_state.default_avatar = bytes.to_vec();
            }
        }
        _ => {
            logger::warn("Failed to fetch default avatar from https://a.ppy.sh/");
        }
    }

    commands::register_commands(&mut app_state);

    let shared_state: state::SharedState = Arc::new(RwLock::new(app_state));

    if let Some(replay_folder) = config.replay_folder() {
        let state_clone = shared_state.clone();
        tokio::spawn(async move {
            watch_replay_folder(state_clone, replay_folder).await;
        });
    }

    // Direct loopback HTTPS setup for -devserver localhost
    let tls_paths = server::tls::get_or_create_certificates(&data_dir);
    if std::env::args().any(|arg| arg == "--trust-cert") {
        if let Ok(ref paths) = tls_paths {
            let _ = server::tls::install_windows_trust(&paths.cert_der);
        }
    }

    if let Ok(ref paths) = tls_paths {
        if let Ok(acceptor) = server::tls::create_tls_acceptor(&paths.cert_pem, &paths.key_pem) {
            let https_state = shared_state.clone();
            tokio::spawn(async move {
                match TcpListener::bind("127.0.0.1:443").await {
                    Ok(listener) => {
                        logger::success("Direct HTTPS server listening on https://127.0.0.1:443 (for -devserver localhost)");
                        loop {
                            let (stream, _) = match listener.accept().await {
                                Ok(s) => s,
                                Err(_) => continue,
                            };
                            let acceptor = acceptor.clone();
                            let state = https_state.clone();

                            tokio::task::spawn(async move {
                                let tls_stream = match acceptor.accept(stream).await {
                                    Ok(s) => s,
                                    Err(_) => return, // Suppress aborted TLS handshakes / abrupt disconnects
                                };
                                let io = TokioIo::new(tls_stream);
                                let service = service_fn(move |req| {
                                    let state = state.clone();
                                    async move {
                                        let resp = handle_request(state, req).await;
                                        Ok::<_, hyper::Error>(resp.into_hyper_response())
                                    }
                                });

                                let _ = http1::Builder::new().serve_connection(io, service).await;
                            });
                        }
                    }
                    Err(e) => {
                        if e.kind() == std::io::ErrorKind::AddrInUse {
                            logger::warn("Port 443 is already in use. Direct HTTPS is disabled; port 5000 is still available.");
                        } else {
                            logger::warn(&format!("Could not bind port 443 for HTTPS ({}). Port 5000 is still available.", e));
                        }
                    }
                }
            });
        }
    }

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("SERVER_PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(5000);
    let addr: SocketAddr = match format!("{}:{}", host, port).parse() {
        Ok(a) => a,
        Err(e) => {
            logger::error(&format!("Invalid SERVER_HOST or SERVER_PORT ({}:{}): {}", host, port, e));
            return Ok(());
        }
    };

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                logger::error(&format!("Port {} is already in use. Is another instance of osu-localserver already running?", port));
            } else {
                logger::error(&format!("Failed to bind server on {}: {}", addr, e));
            }
            return Ok(());
        }
    };

    logger::success(&format!("Server listening on http://{}", addr));
    logger::info(&format!("osu! version target: {}", VERSION));

    tokio::select! {
        _ = async {
            loop {
                let (stream, _) = match listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        logger::error(&format!("Accept error: {}", e));
                        continue;
                    }
                };
                let io = TokioIo::new(stream);
                let state = shared_state.clone();

                tokio::task::spawn(async move {
                    let service = service_fn(move |req| {
                        let state = state.clone();
                        async move {
                            let resp = handle_request(state, req).await;
                            Ok::<_, hyper::Error>(resp.into_hyper_response())
                        }
                    });

                    if let Err(e) = http1::Builder::new().serve_connection(io, service).await {
                        if !e.is_incomplete_message() {
                            logger::error(&format!("Connection error: {}", e));
                        }
                    }
                });
            }
        } => {},
        _ = tokio::signal::ctrl_c() => {
            logger::info("Shutdown signal received. Shutting down...");
        }
    }

    {
        let s = shared_state.read().await;
        let db = s.db.lock().await;
        let _ = db.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");
    }
    logger::success("Server stopped cleanly.");

    Ok(())
}

async fn handle_request(state: state::SharedState, req: hyper::Request<hyper::body::Incoming>) -> Response {
    let start_time = std::time::Instant::now();
    let (parts, incoming_body) = req.into_parts();
    let method = parts.method;
    let uri = parts.uri;
    let headers = parts.headers;
    let path = uri.path().to_string();
    let query_str = uri.query().unwrap_or("");

    let body_bytes = match incoming_body.collect().await {
        Ok(collected) => collected.to_bytes(),
        Err(_) => bytes::Bytes::new(),
    };

    let params = utils::parse_query_string(query_str);

    let osu_token = headers.get("osu-token").and_then(|v| v.to_str().ok()).map(String::from);

    let host = headers.get("host").and_then(|v| v.to_str().ok()).unwrap_or("localhost").split(':').next().unwrap_or("localhost").to_lowercase();

    // Route normalization for -devserver localhost subdomains
    let normalized_path = match host.as_str() {
        "c.localhost" | "c4.localhost" | "c5.localhost" | "c6.localhost" | "ce.localhost" => {
            if path.starts_with("/c") {
                path.clone()
            } else {
                format!("/c{}", path)
            }
        }
        "osu.localhost" => {
            if path.starts_with("/osu") {
                path.clone()
            } else {
                format!("/osu{}", path)
            }
        }
        "a.localhost" => {
            if path.starts_with("/a") {
                path.clone()
            } else {
                format!("/a{}", path)
            }
        }
        "assets.localhost" => {
            if path.starts_with("/assets") {
                path.clone()
            } else {
                format!("/assets{}", path)
            }
        }
        _ => path.clone(),
    };

    let route = match_route(&normalized_path, &params);

    let resp = match route {
        RouteMatch::Cho => handlers::cho::handle(state, osu_token.as_deref(), &body_bytes).await,
        RouteMatch::Web(sub_path) => handlers::web::handle(state, &sub_path, &params, &method, &headers, &body_bytes).await,
        RouteMatch::Avatar(userid) => handlers::avatar::handle(state, userid).await,
        RouteMatch::Api(sub_path) => handlers::api::handle(state, &sub_path, &params).await,
        RouteMatch::Download(setid) => handlers::web::handle_download(state, setid).await,
        RouteMatch::BeatmapWeb(full_path) => Response::redirect(&format!("https://osu.ppy.sh{}", full_path)),
        RouteMatch::Screenshot(link) => Response::redirect(&link),
        RouteMatch::NotFound => Response::not_found(),
    };

    logger::http_request(method.as_str(), &path, resp.status.as_u16(), start_time.elapsed());
    resp
}

async fn watch_replay_folder(state: state::SharedState, replay_folder: std::path::PathBuf) {
    if !replay_folder.exists() {
        logger::error("Replay folder does not exist! Restart server after configuring path.");
        return;
    }

    let mut last_changed =
        replay_folder.metadata().map(|m| m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)).unwrap_or(std::time::SystemTime::UNIX_EPOCH);

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        {
            let s = state.read().await;
            if s.player.is_none() {
                continue;
            }
        }

        let current = replay_folder.metadata().map(|m| m.modified().unwrap_or(std::time::SystemTime::UNIX_EPOCH)).unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        if current == last_changed {
            continue;
        }
        last_changed = current;

        // Brief delay to allow osu! client to finish writing and close the replay file
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let latest_replay = match find_latest_replay(&replay_folder) {
            Some(p) => p,
            None => continue,
        };

        if !latest_replay.exists() {
            logger::warn("Replay file does not exist or was deleted.");
            continue;
        }

        match types::Replay::from_file(&latest_replay) {
            Ok(replay) => {
                let score = types::Score::from_replay(&replay);
                handlers::score_submit::score_submit(state.clone(), score, &replay).await;
            }
            Err(e) => {
                logger::error(&format!("Failed to parse replay file: {}", e));
            }
        }
    }
}

fn find_latest_replay(folder: &std::path::Path) -> Option<std::path::PathBuf> {
    let mut latest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    let entries = std::fs::read_dir(folder).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("osr") {
            continue;
        }
        let modified = entry.metadata().ok().and_then(|m| m.created().or_else(|_| m.modified()).ok()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);

        if latest.as_ref().is_none_or(|(_, t)| modified > *t) {
            latest = Some((path, modified));
        }
    }

    latest.map(|(p, _)| p)
}
