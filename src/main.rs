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

use colored::Colorize;
use http_body_util::BodyExt;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use std::io::Write;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use server::response::Response;
use server::router::{match_route, RouteMatch};
use state::AppState;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const LOGO: &str = r#"                      ,───.                             
                      │   │            ,──.             
 ,───.  ,───. ,──.,──.│  .',───.  ,───.│  ,───.  ,───.  
│ .─. │(  .─' │  ││  ││  ││ .─. :│ .──'│  .─.  ││ .─. │ 
' '─' '.─'  `)'  ''  '`──'╲   ──.╲ `──.│  │ │  │' '─' ' 
 `───' `────'  `────' .──. `────' `───'`──' `──' `───'  
                      '──'     "#;

pub fn clear_console() {
    let mut stdout = std::io::stdout();
    let _ = crossterm::execute!(
        stdout,
        crossterm::terminal::Clear(crossterm::terminal::ClearType::All),
        crossterm::terminal::Clear(crossterm::terminal::ClearType::Purge),
        crossterm::cursor::MoveTo(0, 0)
    );
    print!("\x1B[2J\x1B[3J\x1B[H");
    let _ = stdout.flush();
}

fn print_banner() {
    println!("\n{}", LOGO.magenta().bold());
    println!("  Welcome to {} v{}", "osu-echo".magenta().bold(), VERSION);
    println!("  {}\n", "A local osu! server written in Rust".dimmed());
}

fn print_help() {
    print_banner();
    println!("Usage: osu-echo [OPTIONS]\n");
    println!("Options:");
    println!("  -h, --help           Print help information");
    println!("  -v, --version        Print version information");
    println!("  -d, --debug          Enable verbose debug logging (or hold Shift when starting)");
    println!("  -s, --setup          Run or re-run the interactive setup wizard");
    println!("      --reconfigure    Alias for --setup");
    println!("      --trust-cert     Install and trust the local TLS certificate in Windows Root store");
    println!("      --legacy-tls     Also accept TLS 1.0/1.1 (osu! clients older than 2023). Process-local; Windows' TLS policy is not changed.");
    println!("      --no-hosts-edit  Never modify the Windows hosts file (use when DNS is managed elsewhere)");
}

#[cfg(target_os = "windows")]
fn is_shift_pressed() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(vKey: i32) -> i16;
    }
    const VK_SHIFT: i32 = 0x10;
    unsafe { (GetAsyncKeyState(VK_SHIFT) as u16 & 0x8000) != 0 }
}

#[cfg(not(target_os = "windows"))]
fn is_shift_pressed() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn is_launched_from_explorer() -> bool {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetConsoleProcessList(process_list: *mut u32, count: u32) -> u32;
    }
    let mut pids = [0u32; 2];
    let count = unsafe { GetConsoleProcessList(pids.as_mut_ptr(), 2) };
    count <= 1
}

#[cfg(not(target_os = "windows"))]
fn is_launched_from_explorer() -> bool {
    false
}

fn pause_if_double_clicked() {
    if is_launched_from_explorer() {
        println!("\nPress Enter to exit...");
        let mut buffer = String::new();
        let _ = std::io::stdin().read_line(&mut buffer);
    }
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        logger::error(&format!("Fatal error: {}", e));
        pause_if_double_clicked();
        std::process::exit(1);
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        print_help();
        return Ok(());
    }

    if args.iter().any(|arg| arg == "-v" || arg == "--version") {
        println!("osu-echo v{}", VERSION);
        return Ok(());
    }

    let force_setup = args.iter().any(|arg| arg == "-s" || arg == "--setup" || arg == "--reconfigure");

    dotenvy::dotenv().ok();

    let shift_held = is_shift_pressed();
    let debug_cli = args.iter().any(|arg| arg == "-d" || arg == "--debug" || arg == "--verbose");
    let debug_env = std::env::var("DEBUG").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false)
        || std::env::var("VERBOSE").map(|v| v == "1" || v.eq_ignore_ascii_case("true")).unwrap_or(false);

    let debug_enabled = shift_held || debug_cli || debug_env;
    if debug_enabled {
        logger::set_debug(true);
    }

    let log_debug_info = || {
        if logger::is_debug() {
            let reason = if shift_held {
                "Shift key held at startup"
            } else if debug_cli {
                "CLI flag"
            } else {
                "DEBUG/VERBOSE environment variable"
            };
            logger::info(&format!("Verbose debug logging enabled (via {}).", reason));
        }
    };

    let data_dir = std::env::current_dir()?.join(".data");
    std::fs::create_dir_all(&data_dir)?;

    let db_path = data_dir.join("server.db");
    let conn = rusqlite::Connection::open(&db_path)?;
    db::init_db(&conn)?;

    let mut config = if force_setup {
        let config = types::config::setup_config(&data_dir);
        db::save_config(&conn, &config)?;
        dotenvy::from_filename(".env").ok();
        dotenvy::dotenv().ok();
        clear_console();
        print_banner();
        log_debug_info();
        logger::info("osu-echo is ready! Connect in osu! with: -devserver localhost");
        config
    } else {
        match db::load_config(&conn)? {
            Some(config) => {
                print_banner();
                log_debug_info();
                logger::info("Found existing server configuration.");
                config
            }
            None => {
                let config = types::config::setup_config(&data_dir);
                db::save_config(&conn, &config)?;
                dotenvy::from_filename(".env").ok();
                dotenvy::dotenv().ok();
                clear_console();
                print_banner();
                log_debug_info();
                logger::info("osu-echo is ready! Connect in osu! with: -devserver localhost");
                config
            }
        }
    };

    config.apply_env_overrides();

    if let Some(ref replay_folder) = config.replay_folder() {
        std::fs::create_dir_all(replay_folder).ok();
    }

    let mut app_state = AppState::new(conn, config.clone());

    commands::register_commands(&mut app_state);

    let shared_state: state::SharedState = Arc::new(RwLock::new(app_state));

    let avatar_http = {
        let s = shared_state.read().await;
        s.http.clone()
    };
    let state_avatar = shared_state.clone();
    tokio::spawn(async move {
        let req = avatar_http
            .get("https://a.ppy.sh/")
            .timeout(std::time::Duration::from_secs(2));
        if let Ok(resp) = req.send().await {
            if resp.status() == 200 {
                if let Ok(bytes) = resp.bytes().await {
                    let mut s = state_avatar.write().await;
                    s.default_avatar = bytes.to_vec();
                }
            }
        }
    });

    if let Some(replay_folder) = config.replay_folder() {
        let state_clone = shared_state.clone();
        tokio::spawn(async move {
            watch_replay_folder(state_clone, replay_folder).await;
        });
    }

    // Resolved up front: the legacy TLS front-end forwards to this port, so it has to be known
    // before the HTTPS listener is set up.
    let http_port: u16 = std::env::var("SERVER_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(5000);

    // Direct loopback HTTPS setup for -devserver localhost
    let edit_hosts = !std::env::args().any(|arg| arg == "--no-hosts-edit" || arg == "--no-hosts");
    let ensure_hosts = || {
        if edit_hosts {
            #[cfg(target_os = "windows")]
            let _ = server::tls::setup_windows_hosts();
        }
    };

    let tls_paths = server::tls::get_or_create_certificates(&data_dir);
    if std::env::args().any(|arg| arg == "--trust-cert") {
        if let Ok(ref paths) = tls_paths {
            let _ = server::tls::install_windows_trust(&paths.cert_der);
        }
        ensure_hosts();
    }

    if let Ok(ref paths) = tls_paths {
        ensure_hosts();
        // Re-assert trust on every launch: a previous run may have been interrupted (or the
        // certificate may have been removed), and an untrusted certificate makes the client fail
        // the TLS handshake silently and retry "connecting to server" forever.
        let _ = server::tls::install_windows_trust(&paths.cert_der);

        let legacy_tls = std::env::args().any(|arg| arg == "--legacy-tls");
        let https_port: u16 = std::env::var("OSU_ECHO_HTTPS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(443);

        if legacy_tls {
            logger::warn("Legacy TLS enabled (--legacy-tls): accepting TLS 1.0/1.1 for osu! clients older than 2023.");
            logger::warn("  Those protocols are deprecated and only offer weak cipher suites. Nothing outside this process is affected: Windows' own TLS policy is untouched and the listener is loopback-only.");

            match server::tls::LegacyTlsAcceptor::new(&paths.cert_pem, &paths.key_pem) {
                Ok(acceptor) => {
                    // In-process OpenSSL stack (built with `--features legacy-tls`).
                    logger::success("TLS mode: LEGACY (accepts TLS 1.0, 1.1 and 1.2+) - in-process stack");
                    spawn_https_listener(shared_state.clone(), HttpsStack::Legacy(std::sync::Arc::new(acceptor)));
                }
                Err(in_process_error) => {
                    // Fall back to a Python front-end, which needs no build-time C toolchain.
                    logger::info(&format!("No in-process legacy TLS stack ({}).", in_process_error));
                    match server::tls::spawn_legacy_tls_proxy(https_port, http_port, &paths.cert_pem, &paths.key_pem) {
                        Ok(child) => {
                            logger::success(&format!(
                                "TLS mode: LEGACY (accepts TLS 1.0, 1.1 and 1.2+) - python front-end pid {} on https://127.0.0.1:{} -> http://127.0.0.1:{}",
                                child.id(),
                                https_port,
                                http_port
                            ));
                            std::mem::forget(child); // Keep it running for the lifetime of the server
                        }
                        Err(e) => {
                            logger::error(&format!("Could not start the legacy TLS front-end: {}", e));
                            logger::error(&format!("  Port {} is therefore NOT being served; the osu! client cannot connect. Fix the problem above, or drop --legacy-tls.", https_port));
                        }
                    }
                }
            }
        } else if let Ok(acceptor) = server::tls::create_tls_acceptor(&paths.cert_pem, &paths.key_pem) {
            logger::info("TLS mode: MODERN (TLS 1.2+ only). An osu! client older than 2023 will not connect; restart with --legacy-tls to accept it.");
            spawn_https_listener(shared_state.clone(), HttpsStack::Modern(acceptor));
        }
    }

    // Direct loopback HTTP (port 80) setup for -devserver localhost plain HTTP requests (e.g. b.localhost/thumb)
    let http80_state = shared_state.clone();
    tokio::spawn(async move {
        match TcpListener::bind("127.0.0.1:80").await {
            Ok(listener) => {
                logger::success("Direct HTTP server listening on http://127.0.0.1:80 (for -devserver localhost)");
                loop {
                    let (stream, _) = match listener.accept().await {
                        Ok(s) => s,
                        Err(_) => continue,
                    };
                    let io = TokioIo::new(stream);
                    let state = http80_state.clone();
                    let service = service_fn(move |req| {
                        let state = state.clone();
                        async move {
                            let resp = handle_request(state, req).await;
                            Ok::<_, hyper::Error>(resp.into_hyper_response())
                        }
                    });

                    tokio::task::spawn(async move {
                        let _ = http1::Builder::new().serve_connection(io, service).await;
                    });
                }
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    logger::info("Port 80 is already in use. Direct HTTP is disabled; port 5000 is still available.");
                } else {
                    logger::info(&format!("Could not bind port 80 ({}). Port 5000 is still available.", e));
                }
            }
        }
    });

    let host = std::env::var("SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("SERVER_PORT").ok().and_then(|p| p.parse::<u16>().ok()).unwrap_or(5000);
    let addr: SocketAddr = match format!("{}:{}", host, port).parse() {
        Ok(a) => a,
        Err(e) => {
            logger::error(&format!("Invalid SERVER_HOST or SERVER_PORT ({}:{}): {}", host, port, e));
            pause_if_double_clicked();
            return Ok(());
        }
    };

    let listener = match TcpListener::bind(addr).await {
        Ok(l) => l,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::AddrInUse {
                logger::error(&format!("Port {} is already in use. Is another instance of osu-echo already running?", port));
            } else {
                logger::error(&format!("Failed to bind server on {}: {}", addr, e));
            }
            pause_if_double_clicked();
            return Ok(());
        }
    };

    logger::success(&format!("Server listening on http://{}", addr));
    logger::info(&format!("osu-echo version: v{}", VERSION));
    server::tls::report_client_readiness();

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

/// Which TLS stack serves the loopback HTTPS port.
enum HttpsStack {
    /// rustls: TLS 1.2/1.3 only, the default.
    Modern(tokio_rustls::TlsAcceptor),
    /// OpenSSL with TLS 1.0/1.1 allowed, for osu! builds from 2022 and earlier (`--legacy-tls`).
    Legacy(std::sync::Arc<server::tls::LegacyTlsAcceptor>),
}

/// Serves one already-handshaked connection.
async fn serve_over_tls<S>(state: state::SharedState, io: S)
where
    S: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let service = service_fn(move |req| {
        let state = state.clone();
        async move {
            let resp = handle_request(state, req).await;
            Ok::<_, hyper::Error>(resp.into_hyper_response())
        }
    });

    let _ = http1::Builder::new().serve_connection(io, service).await;
}

/// Binds the loopback HTTPS port and serves whichever TLS stack was selected.
///
/// Kept as one function so both stacks share the accept loop, the bind diagnostics and the fact
/// that a failed handshake is always reported: a client that cannot complete the handshake
/// (untrusted certificate, TLS version it does not speak) never produces an HTTP request, so
/// without that the only symptom is the game saying "connection failed" with a healthy-looking log.
fn spawn_https_listener(state: state::SharedState, stack: HttpsStack) {
    let port: u16 = std::env::var("OSU_ECHO_HTTPS_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(443);

    tokio::spawn(async move {
        let listener = match TcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], port))).await {
            Ok(listener) => listener,
            Err(e) => {
                if e.kind() == std::io::ErrorKind::AddrInUse {
                    logger::error(&format!(
                        "Port {} is already in use, so -devserver localhost cannot connect: the client speaks HTTPS on {} and will sit on \"connecting to server\".",
                        port, port
                    ));
                    logger::error(&format!("  Stop whatever holds port {} ({}) or set OSU_ECHO_HTTPS_PORT to a free port, then retry.", port, e));
                } else {
                    logger::error(&format!("Could not bind port {} for HTTPS ({}). -devserver localhost will not connect.", port, e));
                }
                return;
            }
        };

        logger::success(&format!("Direct HTTPS server listening on https://127.0.0.1:{} (for -devserver localhost)", port));

        loop {
            let (stream, peer) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => continue,
            };
            let state = state.clone();
            let stack = match &stack {
                HttpsStack::Modern(acceptor) => HttpsStackInner::Modern(acceptor.clone()),
                HttpsStack::Legacy(acceptor) => HttpsStackInner::Legacy(acceptor.clone()),
            };

            tokio::task::spawn(async move {
                match stack.handshake(stream).await {
                    Ok(io) => serve_over_tls(state, io).await,
                    Err(e) => {
                        logger::warn(&format!("TLS handshake from {} failed: {}", peer, e));
                        if let Some(hint) = server::tls::explain_handshake_failure(&e) {
                            logger::warn(hint);
                        }
                    }
                }
            });
        }
    });
}

enum HttpsStackInner {
    Modern(tokio_rustls::TlsAcceptor),
    Legacy(std::sync::Arc<server::tls::LegacyTlsAcceptor>),
}

impl HttpsStackInner {
    async fn handshake(self, stream: tokio::net::TcpStream) -> Result<TokioIo<Box<dyn HttpsIo>>, String> {
        match self {
            HttpsStackInner::Modern(acceptor) => {
                let tls = acceptor.accept(stream).await.map_err(|e| e.to_string())?;
                Ok(TokioIo::new(Box::new(tls) as Box<dyn HttpsIo>))
            }
            #[cfg(all(target_os = "windows", feature = "legacy-tls"))]
            HttpsStackInner::Legacy(acceptor) => {
                let ssl = acceptor.ssl();
                let tls = tokio_openssl::SslStream::new(ssl, stream).await.map_err(|e| e.to_string())?;
                Ok(TokioIo::new(Box::new(tls) as Box<dyn HttpsIo>))
            }
            #[cfg(not(all(target_os = "windows", feature = "legacy-tls")))]
            HttpsStackInner::Legacy(_) => Err("this build has no legacy TLS support".to_string()),
        }
    }
}

/// Async byte stream that can sit under hyper, so both TLS stacks share one code path.
trait HttpsIo: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send {}
impl<T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send> HttpsIo for T {}

/// Decides which handler serves a request.
///
/// The osu! client addresses every endpoint through a subdomain of the base domain given to
/// `-devserver` (`c4.` for bancho, `osu!`/`osu.` for the web API, `a.` avatars, `b.` osu!direct,
/// `assets.` covers), so it is the *label* and not the domain that says what a path means. The base
/// domain is the user's choice — `localhost`, a real domain, a tunnel — so it must never be matched
/// by name: with a domain devserver every bancho request used to be answered with the status page,
/// leaving the client with a connection that logs in and chats into the void.
fn resolve_route(host: &str, path: &str, params: &std::collections::HashMap<String, String>, method: &hyper::Method, body_len: usize) -> RouteMatch {
    let is_post_with_body = *method == hyper::Method::POST && body_len > 0;
    let normalized_path = server::router::normalize_client_path(host, path, is_post_with_body);
    match_route(&normalized_path, params)
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

    if logger::is_debug() {
        let q = if query_str.is_empty() { String::new() } else { format!("?{}", query_str) };
        let token_info = match &osu_token {
            Some(t) => format!(", token: {}", t),
            None => String::new(),
        };
        logger::debug(&format!("Incoming HTTP {:<4} {}{} [host: {}{}, body: {}B]", method, path, q, host, token_info, body_bytes.len()));
    }

    let route = resolve_route(&host, &path, &params, &method, body_bytes.len());

    let resp = match route {
        RouteMatch::Status => {
            let html = r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>osu-echo</title>
    <style>
        body {
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, Helvetica, Arial, sans-serif;
            background: #121016;
            color: #f0edf6;
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
            margin: 0;
        }
        .card {
            background: #1e1a24;
            padding: 2.5rem 3rem;
            border-radius: 12px;
            box-shadow: 0 8px 30px rgba(0, 0, 0, 0.5);
            border: 1px solid #332b3d;
            text-align: center;
            max-width: 480px;
        }
        h1 { margin: 0 0 0.5rem; color: #ff66aa; font-size: 2.2rem; letter-spacing: -0.5px; }
        p { color: #a39cb0; font-size: 1.05rem; line-height: 1.5; margin: 0.5rem 0; }
        .badge {
            display: inline-block;
            background: #1c3d25;
            color: #4ade80;
            padding: 0.4rem 1rem;
            border-radius: 9999px;
            font-weight: 600;
            font-size: 0.9rem;
            margin-top: 1.25rem;
            border: 1px solid #225e34;
        }
    </style>
</head>
<body>
    <div class="card">
        <h1>osu-echo</h1>
        <p>A local osu! private server written in Rust.</p>
        <p>The server is running and ready for osu! client connections.</p>
        <div class="badge">&#10003; Server Online</div>
    </div>
</body>
</html>"#;
            Response::html(html)
        }
        RouteMatch::Favicon => Response::no_content().with_header("Content-Type", "image/x-icon"),
        RouteMatch::Cho => handlers::cho::handle(state, osu_token.as_deref(), &body_bytes).await,
        RouteMatch::Web(sub_path) => handlers::web::handle(state, &sub_path, &params, &method, &headers, &body_bytes).await,
        RouteMatch::Avatar(userid) => handlers::avatar::handle(state, userid).await,
        RouteMatch::Api(sub_path) => handlers::api::handle(state, &sub_path, &params).await,
        RouteMatch::Download(setid, no_video) => handlers::web::handle_download(state, setid, no_video).await,
        RouteMatch::Thumbnail(filename) => handlers::web::handle_thumbnail(state, &filename).await,
        RouteMatch::Preview(filename) => handlers::web::handle_preview(state, &filename).await,
        RouteMatch::Asset(sub_path) => handlers::web::handle_asset(state, &sub_path).await,
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

        if logger::is_debug() {
            logger::debug(&format!("Replay file detected: {}", latest_replay.display()));
        }

        if !latest_replay.exists() {
            logger::warn("Replay file does not exist or was deleted.");
            continue;
        }

        match types::Replay::from_file(&latest_replay) {
            Ok(replay) => {
                if logger::is_debug() {
                    logger::debug(&format!(
                        "Parsed replay: player={}, map_md5={}, score={}, combo={}",
                        replay.player_name, replay.beatmap_md5, replay.total_score, replay.combo
                    ));
                }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// The client POSTs its bancho packet stream to `/` of the `c*` subdomain of whatever
    /// `-devserver` was given. Regression test for a server used through a domain devserver,
    /// where those requests used to resolve to the status page instead of the packet handler.
    #[test]
    fn test_bancho_traffic_is_routed_for_any_devserver_domain() {
        let params = HashMap::new();
        for host in ["c4.catboy.click", "c.catboy.click", "c5.catboy.click", "c6.catboy.click", "ce.catboy.click", "cho.localhost", "c1.localhost", "c4.localhost"] {
            assert_eq!(resolve_route(host, "/", &params, &hyper::Method::POST, 128), RouteMatch::Cho, "bancho broken for {}", host);
        }
        // Same for a tunnel that rewrites Host to the upstream address.
        assert_eq!(resolve_route("127.0.0.1", "/", &params, &hyper::Method::POST, 128), RouteMatch::Cho);
    }

    #[test]
    fn test_web_and_direct_routes_for_any_devserver_domain() {
        let params = HashMap::new();
        let cases = [
            ("osu!.catboy.click", "/web/osu-search.php", RouteMatch::Web("/osu-search.php".to_string())),
            ("osu.catboy.click", "/web/osu-search.php", RouteMatch::Web("/osu-search.php".to_string())),
            ("osu!.catboy.click", "/web/bancho_connect.php", RouteMatch::Web("/bancho_connect.php".to_string())),
            ("b.catboy.click", "/d/2620610", RouteMatch::Download(2620610, false)),
            ("b.catboy.click", "/thumb/2620610l.jpg", RouteMatch::Thumbnail("2620610l.jpg".to_string())),
            ("a.catboy.click", "/4", RouteMatch::Avatar(4)),
        ];

        for (host, path, expected) in cases {
            assert_eq!(resolve_route(host, path, &params, &hyper::Method::GET, 0), expected, "route failed for {}{}", host, path);
        }
    }

    /// The status page is only for a human opening the site, never for client traffic.
    #[test]
    fn test_status_page_is_not_served_to_the_client() {
        let params = HashMap::new();
        assert_eq!(resolve_route("catboy.click", "/", &params, &hyper::Method::GET, 0), RouteMatch::Status);
        assert_eq!(resolve_route("127.0.0.1:5000", "/", &params, &hyper::Method::GET, 0), RouteMatch::Status);
        assert_ne!(resolve_route("c4.catboy.click", "/", &params, &hyper::Method::POST, 7), RouteMatch::Status);
    }
}
