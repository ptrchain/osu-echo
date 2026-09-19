use md5::Digest;
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MenuIcon {
    pub image_link: Option<String>,
    pub click_link: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Paths {
    pub osu_path: Option<String>,
    pub songs: Option<String>,
    pub replay: Option<String>,
    pub screenshots: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub paths: Paths,
    pub pp_leaderboard: bool,
    pub ping_user_when_recent_score: bool,
    #[serde(default = "default_true")]
    pub enable_recent_channel: bool,
    pub menu_icon: MenuIcon,
    pub command_prefix: String,
    pub show_pp_for_personal_best: bool,
    pub amount_of_scores_on_lb: i32,
    pub auto_update: bool,
    pub disable_funorange_maps: bool,
    pub osu_api_key: Option<String>,
    pub osu_daily_api_key: Option<String>,
    pub seasonal_bgs: Vec<String>,
    pub osu_username: Option<String>,
    pub osu_password: Option<String>,
    pub country: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            paths: Paths { osu_path: None, songs: None, replay: None, screenshots: None },
            pp_leaderboard: false,
            ping_user_when_recent_score: false,
            enable_recent_channel: true,
            menu_icon: MenuIcon { image_link: None, click_link: None },
            command_prefix: "!".to_string(),
            show_pp_for_personal_best: false,
            amount_of_scores_on_lb: 50,
            auto_update: true,
            disable_funorange_maps: false,
            osu_api_key: None,
            osu_daily_api_key: None,
            seasonal_bgs: vec![],
            osu_username: None,
            osu_password: None,
            country: None,
        }
    }
}

impl Config {
    pub fn songs_folder(&self) -> Option<PathBuf> {
        self.paths.songs.as_ref().map(PathBuf::from).or_else(|| self.paths.osu_path.as_ref().map(|osu| PathBuf::from(osu).join("Songs")))
    }

    pub fn replay_folder(&self) -> Option<PathBuf> {
        self.paths.replay.as_ref().map(PathBuf::from).or_else(|| self.paths.osu_path.as_ref().map(|osu| PathBuf::from(osu).join("Replays")))
    }

    pub fn screenshots_folder(&self) -> Option<PathBuf> {
        self.paths.screenshots.as_ref().map(PathBuf::from).or_else(|| self.paths.osu_path.as_ref().map(|osu| PathBuf::from(osu).join("Screenshots")))
    }

    pub fn apply_env_overrides(&mut self) {
        if let Ok(val) = std::env::var("OSU_USERNAME") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.osu_username = Some(trimmed.to_string());
            }
        }

        // Support pre-hashed password or raw password in env
        if let Ok(val) = std::env::var("OSU_PASSWORD_HASH") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.osu_password = Some(trimmed.to_string());
            }
        } else if let Ok(raw_pw) = std::env::var("OSU_PASSWORD") {
            let trimmed = raw_pw.trim();
            if !trimmed.is_empty() {
                let hash = format!("{:x}", md5::Md5::digest(trimmed.as_bytes()));
                self.osu_password = Some(hash);
            }
        }

        if let Ok(val) = std::env::var("OSU_API_KEY") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.osu_api_key = Some(trimmed.to_string());
            }
        }

        if let Ok(val) = std::env::var("OSU_DAILY_API_KEY") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.osu_daily_api_key = Some(trimmed.to_string());
            }
        }

        if let Ok(val) = std::env::var("PP_LEADERBOARD") {
            let trimmed = val.trim().to_lowercase();
            if trimmed == "true" || trimmed == "1" || trimmed == "yes" {
                self.pp_leaderboard = true;
            } else if trimmed == "false" || trimmed == "0" || trimmed == "no" {
                self.pp_leaderboard = false;
            }
        }

        if let Ok(val) = std::env::var("SHOW_PP_FOR_PERSONAL_BEST") {
            let trimmed = val.trim().to_lowercase();
            if trimmed == "true" || trimmed == "1" || trimmed == "yes" {
                self.show_pp_for_personal_best = true;
            } else if trimmed == "false" || trimmed == "0" || trimmed == "no" {
                self.show_pp_for_personal_best = false;
            }
        }

        if let Ok(val) = std::env::var("AMOUNT_OF_SCORES_ON_LB") {
            if let Ok(amount) = val.trim().parse::<i32>() {
                self.amount_of_scores_on_lb = amount.clamp(1, 100);
            }
        }

        if let Ok(val) = std::env::var("ENABLE_RECENT_CHANNEL").or_else(|_| std::env::var("RECENT_CHANNEL")) {
            let trimmed = val.trim().to_lowercase();
            if trimmed == "true" || trimmed == "1" || trimmed == "yes" {
                self.enable_recent_channel = true;
            } else if trimmed == "false" || trimmed == "0" || trimmed == "no" {
                self.enable_recent_channel = false;
            }
        }

        if let Ok(val) = std::env::var("COUNTRY") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.country = Some(trimmed.to_uppercase());
            }
        }
    }
}

pub fn detect_osu_path() -> Option<PathBuf> {
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let candidate = PathBuf::from(local_app_data).join("osu!");
        if candidate.exists() && candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

pub fn write_dotenv_country(country: &str) -> std::io::Result<()> {
    let env_path = Path::new(".env");
    let mut map = std::collections::BTreeMap::new();

    if env_path.exists() {
        if let Ok(content) = std::fs::read_to_string(env_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once('=') {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }

    map.insert("COUNTRY".to_string(), country.to_uppercase());

    map.entry("SERVER_HOST".to_string()).or_insert_with(|| "127.0.0.1".to_string());
    map.entry("SERVER_PORT".to_string()).or_insert_with(|| "5000".to_string());

    let mut out = String::from("# osu! Local Server - Environment Configuration\n");
    for (k, v) in &map {
        out.push_str(&format!("{}={}\n", k, v));
    }

    std::fs::write(env_path, out)?;
    Ok(())
}

pub fn write_dotenv(
    username: Option<&str>,
    password_hash: Option<&str>,
    api_key: Option<&str>,
    daily_key: Option<&str>,
    pp_leaderboard: Option<bool>,
    show_pp_for_personal_best: Option<bool>,
    amount_of_scores: Option<i32>,
) -> std::io::Result<()> {
    let env_path = Path::new(".env");
    let mut map = std::collections::BTreeMap::new();

    if env_path.exists() {
        if let Ok(content) = std::fs::read_to_string(env_path) {
            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.starts_with('#') || trimmed.is_empty() {
                    continue;
                }
                if let Some((k, v)) = trimmed.split_once('=') {
                    map.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }

    if let Some(u) = username {
        map.insert("OSU_USERNAME".to_string(), u.to_string());
    }
    if let Some(h) = password_hash {
        map.insert("OSU_PASSWORD_HASH".to_string(), h.to_string());
    }
    if let Some(k) = api_key {
        map.insert("OSU_API_KEY".to_string(), k.to_string());
    }
    if let Some(d) = daily_key {
        map.insert("OSU_DAILY_API_KEY".to_string(), d.to_string());
    }
    if let Some(pp) = pp_leaderboard {
        map.insert("PP_LEADERBOARD".to_string(), pp.to_string());
    }
    if let Some(pb) = show_pp_for_personal_best {
        map.insert("SHOW_PP_FOR_PERSONAL_BEST".to_string(), pb.to_string());
    }
    if let Some(amt) = amount_of_scores {
        map.insert("AMOUNT_OF_SCORES_ON_LB".to_string(), amt.to_string());
    }

    map.entry("SERVER_HOST".to_string()).or_insert_with(|| "127.0.0.1".to_string());
    map.entry("SERVER_PORT".to_string()).or_insert_with(|| "5000".to_string());

    let mut out = String::from("# osu! Local Server - Environment Configuration\n");
    out.push_str(&format!("SERVER_HOST={}\n", map.get("SERVER_HOST").unwrap()));
    out.push_str(&format!("SERVER_PORT={}\n\n", map.get("SERVER_PORT").unwrap()));

    out.push_str("# External API Keys (Recommended)\n");
    out.push_str("# Legacy v1 API key for online beatmap leaderboards: https://old.ppy.sh/p/api/\n");
    out.push_str(&format!("OSU_API_KEY={}\n", map.get("OSU_API_KEY").cloned().unwrap_or_default()));
    out.push_str("# osudaily API key for real-time global rank calculation: https://osudaily.net/api.php\n");
    out.push_str(&format!("OSU_DAILY_API_KEY={}\n\n", map.get("OSU_DAILY_API_KEY").cloned().unwrap_or_default()));

    out.push_str("# Optional osu! Account Credentials (only needed for '!friend sync')\n");
    out.push_str(&format!("OSU_USERNAME={}\n", map.get("OSU_USERNAME").cloned().unwrap_or_default()));
    out.push_str(&format!("OSU_PASSWORD_HASH={}\n\n", map.get("OSU_PASSWORD_HASH").cloned().unwrap_or_default()));

    out.push_str("# Leaderboard Settings\n");
    out.push_str("# Show PP instead of Score on beatmap leaderboards\n");
    out.push_str(&format!("PP_LEADERBOARD={}\n", map.get("PP_LEADERBOARD").cloned().unwrap_or_else(|| "false".to_string())));
    out.push_str("# Show PP in the personal best score panel\n");
    out.push_str(&format!("SHOW_PP_FOR_PERSONAL_BEST={}\n", map.get("SHOW_PP_FOR_PERSONAL_BEST").cloned().unwrap_or_else(|| "false".to_string())));
    out.push_str("# Number of scores shown on leaderboards (1-100)\n");
    out.push_str(&format!("AMOUNT_OF_SCORES_ON_LB={}\n", map.get("AMOUNT_OF_SCORES_ON_LB").cloned().unwrap_or_else(|| "50".to_string())));

    if let Some(country) = map.get("COUNTRY") {
        out.push_str("\n# Player Profile Settings\n");
        out.push_str(&format!("COUNTRY={}\n", country));
    }

    if let Some(recent) = map.get("ENABLE_RECENT_CHANNEL") {
        out.push_str("\n# Chat Feed Settings\n");
        out.push_str(&format!("ENABLE_RECENT_CHANNEL={}\n", recent));
    }

    std::fs::write(env_path, out)?;
    Ok(())
}

pub fn setup_config(data_dir: &Path) -> Config {
    let mut config = Config::default();

    println!("\n=== osu! Local Server Quick Setup ===");
    println!("Press Enter to accept recommended defaults.\n");

    let detected_osu = detect_osu_path();
    let prompt = match &detected_osu {
        Some(path) => format!("osu! folder path [default: {}]:", path.to_string_lossy()),
        None => "osu! folder path:".to_string(),
    };

    loop {
        let input = prompt_input(&prompt);
        if input.is_empty() {
            if let Some(ref detected) = detected_osu {
                config.paths.osu_path = Some(detected.to_string_lossy().to_string());
                break;
            } else {
                println!("No osu! path provided. Path-dependent features will be disabled.");
                break;
            }
        }

        let p = PathBuf::from(&input);
        if !p.exists() {
            println!("Directory '{}' does not exist! Please try again.", input);
            continue;
        }
        config.paths.osu_path = Some(input);
        break;
    }

    if config.paths.osu_path.is_some() {
        let customize_subfolders = prompt_bool("Customize Songs/Replays/Screenshots subfolders? (y/N) [default: no]", false);
        if customize_subfolders {
            config.paths.songs = prompt_optional_path("Songs folder path (or Enter for default)");
            config.paths.replay = prompt_optional_path("Replays folder path (or Enter for default)");
            config.paths.screenshots = prompt_optional_path("Screenshots folder path (or Enter for default)");
        }
    }

    println!("\n--- External API Keys (Essential for Leaderboards & Rank) ---");
    println!("1. osu! Legacy API Key (v1):");
    println!("   Required for online beatmap leaderboards & global scores comparison.");
    println!("   Get yours at: https://old.ppy.sh/p/api/");
    config.osu_api_key = prompt_optional_string("osu! legacy API key (or Enter to skip):");

    println!("\n2. osu!daily API Key:");
    println!("   Required for real-time global rank calculation based on your PP.");
    println!("   Get yours at: https://osudaily.net/api.php");
    config.osu_daily_api_key = prompt_optional_string("osu!daily API key (or Enter to skip):");

    println!("\n--- osu! Account Credentials (Optional) ---");
    println!("In-game search and downloads work via Catboy mirror without credentials.");
    println!("Credentials are only needed if you want to sync your friends list (!friend sync).");
    let setup_creds = prompt_bool("Configure official osu! account credentials? (y/N) [default: no]", false);
    if setup_creds {
        let username = prompt_optional_string("osu! username (or Enter to skip):");
        if let Some(ref u) = username {
            let raw_pw = prompt_password("osu! password (will be MD5-hashed, or Enter to skip) [Hold Tab to reveal]:");
            if let Some(pw) = raw_pw {
                let hash = format!("{:x}", md5::Md5::digest(pw.as_bytes()));
                config.osu_password = Some(hash);
                println!("Password hashed successfully (plaintext password is never stored).");
            }
            config.osu_username = Some(u.clone());
        }
    }

    println!("\n--- Leaderboard Settings ---");
    config.pp_leaderboard = prompt_bool("Show PP instead of Score on leaderboards? (y/N) [default: no]", false);
    config.show_pp_for_personal_best = prompt_bool("Show PP for personal best score panel? (y/N) [default: no]", false);

    #[cfg(target_os = "windows")]
    {
        println!("\n--- HTTPS Certificate Setup (-devserver localhost) ---");
        println!("osu! requires a trusted certificate to connect using '-devserver localhost'.");
        let install_cert = prompt_bool("Install & trust local certificate for osu! client? (Y/n) [default: yes]", true);
        if install_cert {
            match crate::server::tls::get_or_create_certificates(data_dir) {
                Ok(paths) => {
                    if let Err(e) = crate::server::tls::install_windows_trust(&paths.cert_der) {
                        crate::logger::warn(&format!("Failed to install certificate: {}", e));
                    }
                }
                Err(e) => {
                    crate::logger::warn(&format!("Failed to generate certificate: {}", e));
                }
            }
        } else {
            println!("Skipping certificate installation. You can run with '--trust-cert' later.");
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = data_dir;
    }

    if let Err(e) = write_dotenv(
        config.osu_username.as_deref(),
        config.osu_password.as_deref(),
        config.osu_api_key.as_deref(),
        config.osu_daily_api_key.as_deref(),
        Some(config.pp_leaderboard),
        Some(config.show_pp_for_personal_best),
        Some(config.amount_of_scores_on_lb),
    ) {
        crate::logger::warn(&format!("Could not write .env file: {}", e));
    } else {
        crate::logger::success("Configuration saved to .env");
    }

    println!("\nConfiguration complete!\n");
    config
}

fn prompt_input(prompt: &str) -> String {
    print!("{} ", prompt);
    std::io::stdout().flush().ok();
    let mut input = String::new();
    if std::io::stdin().read_line(&mut input).is_err() {
        return String::new();
    }
    input.trim().to_string()
}

fn prompt_bool(prompt: &str, default_val: bool) -> bool {
    let input = prompt_input(prompt);
    if input.is_empty() {
        return default_val;
    }
    let lower = input.to_lowercase();
    lower.starts_with('y')
}

struct RawModeGuard;
impl Drop for RawModeGuard {
    fn drop(&mut self) {
        let _ = crossterm::terminal::disable_raw_mode();
    }
}

#[cfg(target_os = "windows")]
fn is_tab_down() -> bool {
    #[link(name = "user32")]
    extern "system" {
        fn GetAsyncKeyState(v_key: i32) -> i16;
    }
    // 0x09 = VK_TAB
    unsafe { (GetAsyncKeyState(0x09) as u16 & 0x8000) != 0 }
}

#[cfg(not(target_os = "windows"))]
fn is_tab_down() -> bool {
    false
}

pub fn prompt_password(prompt: &str) -> Option<String> {
    use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
    use std::io::{stdout, IsTerminal, Write};
    use std::time::{Duration, Instant};

    if !std::io::stdin().is_terminal() {
        return prompt_optional_string(prompt);
    }

    print!("{} ", prompt);
    let _ = stdout().flush();

    if crossterm::terminal::enable_raw_mode().is_err() {
        return prompt_optional_string(prompt);
    }
    let _guard = RawModeGuard;

    let mut password = String::new();
    let mut last_rendered_state = String::new();
    let mut tab_held_until: Option<Instant> = None;

    let render = |pw: &str, is_revealed: bool| {
        let mut out = stdout();
        let display = if is_revealed {
            format!("{} (revealed)", pw)
        } else {
            "*".repeat(pw.chars().count())
        };
        let _ = write!(out, "\r\x1b[2K{} {}", prompt, display);
        let _ = out.flush();
    };

    render(&password, false);

    loop {
        let now = Instant::now();

        let is_revealed = !password.is_empty()
            && (is_tab_down() || tab_held_until.is_some_and(|until| now < until));

        let state_key = format!("{}:{}", password, is_revealed);
        if state_key != last_rendered_state {
            render(&password, is_revealed);
            last_rendered_state = state_key;
        }

        let poll_timeout = if is_revealed {
            Duration::from_millis(20)
        } else {
            Duration::from_millis(30)
        };

        if event::poll(poll_timeout).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.modifiers.contains(KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('d'))
                {
                    drop(_guard);
                    println!();
                    return None;
                }

                match key.code {
                    KeyCode::Tab | KeyCode::BackTab => {
                        if key.kind == KeyEventKind::Release {
                            tab_held_until = None;
                        } else {
                            tab_held_until = Some(now + Duration::from_millis(150));
                        }
                    }
                    KeyCode::Enter => {
                        if key.kind == KeyEventKind::Press {
                            break;
                        }
                    }
                    KeyCode::Backspace if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat => {
                        password.pop();
                        last_rendered_state.clear();
                    }
                    KeyCode::Char(c) if key.kind == KeyEventKind::Press || key.kind == KeyEventKind::Repeat => {
                        password.push(c);
                        last_rendered_state.clear();
                    }
                    _ => {}
                }
            }
        }
    }

    drop(_guard);
    println!();

    let trimmed = password.trim().to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn prompt_optional_string(prompt: &str) -> Option<String> {
    let input = prompt_input(prompt);
    if input.is_empty() {
        None
    } else {
        Some(input)
    }
}

fn prompt_optional_path(prompt: &str) -> Option<String> {
    loop {
        let input = prompt_input(prompt);
        if input.is_empty() {
            return None;
        }
        let p = PathBuf::from(&input);
        if !p.exists() {
            println!("Path '{}' does not exist! Please try again.", input);
            continue;
        }
        return Some(input);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_paths_default_resolution() {
        let mut config = Config::default();
        config.paths.osu_path = Some("C:\\osu".to_string());

        assert_eq!(config.songs_folder(), Some(PathBuf::from("C:\\osu\\Songs")));
        assert_eq!(config.replay_folder(), Some(PathBuf::from("C:\\osu\\Replays")));
        assert_eq!(config.screenshots_folder(), Some(PathBuf::from("C:\\osu\\Screenshots")));
    }

    #[test]
    fn test_custom_paths_override_osu_path() {
        let mut config = Config::default();
        config.paths.osu_path = Some("C:\\osu".to_string());
        config.paths.songs = Some("D:\\CustomSongs".to_string());

        assert_eq!(config.songs_folder(), Some(PathBuf::from("D:\\CustomSongs")));
        assert_eq!(config.replay_folder(), Some(PathBuf::from("C:\\osu\\Replays")));
    }

    #[test]
    fn test_env_overrides_and_password_hashing() {
        std::env::set_var("OSU_USERNAME", "testuser");
        std::env::set_var("OSU_PASSWORD", "secret123");
        std::env::set_var("OSU_API_KEY", "apikey_xyz");
        std::env::set_var("OSU_DAILY_API_KEY", "dailykey_123");
        std::env::set_var("PP_LEADERBOARD", "true");
        std::env::set_var("SHOW_PP_FOR_PERSONAL_BEST", "true");
        std::env::set_var("AMOUNT_OF_SCORES_ON_LB", "75");
        std::env::set_var("ENABLE_RECENT_CHANNEL", "false");

        let mut config = Config::default();
        assert!(!config.pp_leaderboard);
        assert!(!config.show_pp_for_personal_best);
        assert!(config.enable_recent_channel);

        config.apply_env_overrides();

        assert_eq!(config.osu_username.as_deref(), Some("testuser"));
        assert_eq!(config.osu_api_key.as_deref(), Some("apikey_xyz"));
        assert_eq!(config.osu_daily_api_key.as_deref(), Some("dailykey_123"));
        assert!(config.pp_leaderboard);
        assert!(config.show_pp_for_personal_best);
        assert_eq!(config.amount_of_scores_on_lb, 75);
        assert!(!config.enable_recent_channel);

        let expected_hash = format!("{:x}", md5::Md5::digest(b"secret123"));
        assert_eq!(config.osu_password.as_deref(), Some(expected_hash.as_str()));

        std::env::remove_var("OSU_USERNAME");
        std::env::remove_var("OSU_PASSWORD");
        std::env::remove_var("OSU_API_KEY");
        std::env::remove_var("OSU_DAILY_API_KEY");
        std::env::remove_var("PP_LEADERBOARD");
        std::env::remove_var("SHOW_PP_FOR_PERSONAL_BEST");
        std::env::remove_var("AMOUNT_OF_SCORES_ON_LB");
        std::env::remove_var("ENABLE_RECENT_CHANNEL");
    }
}
