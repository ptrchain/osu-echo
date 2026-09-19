use crate::types::beatmap::Beatmap;
use crate::types::command::Command;
use crate::types::config::Config;
use crate::types::mods::Mods;
use crate::types::player::Player;
use reqwest::Client as HttpClient;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct AppState {
    pub db: Arc<Mutex<Connection>>,
    pub http: HttpClient,
    pub config: Config,

    pub player: Option<Player>,
    pub mode: Option<Mods>,
    pub invalid_mods: Mods,
    pub current_cmd: Option<Command>,
    pub pending_login_name: Option<String>,
    pub commands: HashMap<String, Command>,
    pub default_avatar: Vec<u8>,
    pub last_np_map: Option<Beatmap>,
    pub recent_recommendations: Vec<String>,
    pub bancho_score_cache: HashMap<(i64, i32, Option<u32>, i32), (std::time::Instant, Vec<crate::types::score::BanchoScore>)>,
}

impl AppState {
    pub fn new(db: Connection, config: Config) -> Self {
        let http = HttpClient::builder()
            .user_agent("osu!")
            .build()
            .unwrap_or_else(|_| HttpClient::new());

        Self {
            db: Arc::new(Mutex::new(db)),
            http,
            config,
            player: None,
            mode: None,
            invalid_mods: Mods::INVALID_STANDARD,
            current_cmd: None,
            pending_login_name: None,
            commands: HashMap::new(),
            default_avatar: Vec::new(),
            last_np_map: None,
            recent_recommendations: Vec::new(),
            bancho_score_cache: HashMap::new(),
        }
    }
}

pub type SharedState = Arc<RwLock<AppState>>;
