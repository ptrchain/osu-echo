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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub paths: Paths,
    pub pp_leaderboard: bool,
    pub ping_user_when_recent_score: bool,
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
}

impl Default for Config {
    fn default() -> Self {
        Self {
            paths: Paths { osu_path: None, songs: None, replay: None, screenshots: None },
            pp_leaderboard: true,
            ping_user_when_recent_score: false,
            menu_icon: MenuIcon { image_link: None, click_link: None },
            command_prefix: "!".to_string(),
            show_pp_for_personal_best: true,
            amount_of_scores_on_lb: 50,
            auto_update: true,
            disable_funorange_maps: false,
            osu_api_key: None,
            osu_daily_api_key: None,
            seasonal_bgs: vec![],
            osu_username: None,
            osu_password: None,
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

    /// Overrides credentials and API keys with values from environment variables / .env
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

/// Helper to write credentials & API keys directly to .env
pub fn write_dotenv(username: Option<&str>, password_hash: Option<&str>, api_key: Option<&str>, daily_key: Option<&str>) -> std::io::Result<()> {
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

    map.entry("SERVER_HOST".to_string()).or_insert_with(|| "127.0.0.1".to_string());
    map.entry("SERVER_PORT".to_string()).or_insert_with(|| "5000".to_string());

    let mut out = String::from("# osu! Local Server - Environment Configuration\n");
    out.push_str(&format!("SERVER_HOST={}\n", map.get("SERVER_HOST").unwrap()));
    out.push_str(&format!("SERVER_PORT={}\n\n", map.get("SERVER_PORT").unwrap()));

    out.push_str("# osu!direct Credentials (password is stored as MD5 hash)\n");
    out.push_str(&format!("OSU_USERNAME={}\n", map.get("OSU_USERNAME").cloned().unwrap_or_default()));
    out.push_str(&format!("OSU_PASSWORD_HASH={}\n\n", map.get("OSU_PASSWORD_HASH").cloned().unwrap_or_default()));

    out.push_str("# External API Keys\n");
    out.push_str(&format!("OSU_API_KEY={}\n", map.get("OSU_API_KEY").cloned().unwrap_or_default()));
    out.push_str(&format!("OSU_DAILY_API_KEY={}\n", map.get("OSU_DAILY_API_KEY").cloned().unwrap_or_default()));

    std::fs::write(env_path, out)?;
    Ok(())
}

/// Streamlined interactive setup flow
pub fn setup_config() -> Config {
    let mut config = Config::default();

    println!("\n=== osu! Local Server Quick Setup ===");
    println!("Press Enter to accept recommended defaults.\n");

    // 1. osu! folder path with auto-detection
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

    // 2. Custom subfolders (only prompt if user wants to customize)
    if config.paths.osu_path.is_some() {
        let customize_subfolders = prompt_bool("Customize Songs/Replays/Screenshots subfolders? (y/N) [default: no]", false);
        if customize_subfolders {
            config.paths.songs = prompt_optional_path("Songs folder path (or Enter for default)");
            config.paths.replay = prompt_optional_path("Replays folder path (or Enter for default)");
            config.paths.screenshots = prompt_optional_path("Screenshots folder path (or Enter for default)");
        }
    }

    // 3. Credentials & API Keys for .env
    println!("\n--- Credentials & API Keys (.env) ---");
    let setup_creds = prompt_bool("Configure osu! credentials & API keys now? (Y/n) [default: yes]", true);

    if setup_creds {
        let username = prompt_optional_string("osu! username (for osu!direct search, or Enter to skip)");
        let mut password_hash = None;

        if let Some(ref u) = username {
            let raw_pw = prompt_optional_string("osu! password (will be MD5-hashed, or Enter to skip)");
            if let Some(pw) = raw_pw {
                let hash = format!("{:x}", md5::Md5::digest(pw.as_bytes()));
                password_hash = Some(hash);
                println!("Password hashed successfully (plaintext password is never stored).");
            }
            config.osu_username = Some(u.clone());
            config.osu_password = password_hash.clone();
        }

        let osu_api_key = prompt_optional_string("osu! API key (https://old.ppy.sh/p/api/, or Enter to skip)");
        let osu_daily_key = prompt_optional_string("osu! daily API key (https://osudaily.net/api/, or Enter to skip)");

        config.osu_api_key = osu_api_key.clone();
        config.osu_daily_api_key = osu_daily_key.clone();

        if let Err(e) = write_dotenv(username.as_deref(), password_hash.as_deref(), osu_api_key.as_deref(), osu_daily_key.as_deref()) {
            crate::logger::warn(&format!("Could not write .env file: {}", e));
        } else {
            crate::logger::success("Credentials and API keys saved to .env");
        }
    } else {
        println!("Skipping credentials. You can set them anytime in the .env file.");
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

fn prompt_optional_string(prompt: &str) -> Option<String> {
    let input = prompt_input(&format!("{}:", prompt));
    if input.is_empty() || input.eq_ignore_ascii_case("none") {
        None
    } else {
        Some(input)
    }
}

fn prompt_optional_path(prompt: &str) -> Option<String> {
    loop {
        let input = prompt_input(&format!("{}:", prompt));
        if input.is_empty() || input.eq_ignore_ascii_case("none") {
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

        let mut config = Config::default();
        config.apply_env_overrides();

        assert_eq!(config.osu_username.as_deref(), Some("testuser"));
        assert_eq!(config.osu_api_key.as_deref(), Some("apikey_xyz"));
        assert_eq!(config.osu_daily_api_key.as_deref(), Some("dailykey_123"));

        let expected_hash = format!("{:x}", md5::Md5::digest(b"secret123"));
        assert_eq!(config.osu_password.as_deref(), Some(expected_hash.as_str()));

        std::env::remove_var("OSU_USERNAME");
        std::env::remove_var("OSU_PASSWORD");
        std::env::remove_var("OSU_API_KEY");
        std::env::remove_var("OSU_DAILY_API_KEY");
    }
}
