use colored::Colorize;
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
    #[serde(default)]
    pub osu_client_id: Option<String>,
    #[serde(default)]
    pub osu_client_secret: Option<String>,
    pub seasonal_bgs: Vec<String>,
    pub osu_username: Option<String>,
    pub osu_password: Option<String>,
    pub country: Option<String>,
    #[serde(default)]
    pub discord_webhook_url: Option<String>,
    #[serde(default)]
    pub discord_webhook_min_pp: Option<f64>,
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
            osu_client_id: None,
            osu_client_secret: None,
            seasonal_bgs: vec![],
            osu_username: None,
            osu_password: None,
            country: None,
            discord_webhook_url: None,
            discord_webhook_min_pp: None,
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

        if let Ok(val) = std::env::var("OSU_CLIENT_ID") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.osu_client_id = Some(trimmed.to_string());
            }
        }

        if let Ok(val) = std::env::var("OSU_CLIENT_SECRET") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.osu_client_secret = Some(trimmed.to_string());
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

        if let Ok(val) = std::env::var("DISCORD_WEBHOOK_URL") {
            let trimmed = val.trim();
            if !trimmed.is_empty() {
                self.discord_webhook_url = Some(trimmed.to_string());
            }
        }

        if let Ok(val) = std::env::var("DISCORD_WEBHOOK_MIN_PP") {
            if let Ok(min_pp) = val.trim().parse::<f64>() {
                self.discord_webhook_min_pp = Some(min_pp);
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
    write_dotenv_full(None, None, Some(country), None, None, None, None, None, None, None, None, None, None)
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
    write_dotenv_full(
        None,
        None,
        None,
        username,
        password_hash,
        api_key,
        daily_key,
        None,
        None,
        pp_leaderboard,
        show_pp_for_personal_best,
        amount_of_scores,
        None,
    )
}

pub fn write_dotenv_full(
    host: Option<&str>,
    port: Option<u16>,
    country: Option<&str>,
    username: Option<&str>,
    password_hash: Option<&str>,
    api_key: Option<&str>,
    daily_key: Option<&str>,
    client_id: Option<&str>,
    client_secret: Option<&str>,
    pp_leaderboard: Option<bool>,
    show_pp_for_personal_best: Option<bool>,
    amount_of_scores: Option<i32>,
    enable_recent_channel: Option<bool>,
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

    if let Some(h) = host {
        map.insert("SERVER_HOST".to_string(), h.to_string());
    }
    if let Some(p) = port {
        map.insert("SERVER_PORT".to_string(), p.to_string());
    }
    if let Some(c) = country {
        map.insert("COUNTRY".to_string(), c.to_uppercase());
    }
    if let Some(u) = username {
        map.insert("OSU_USERNAME".to_string(), u.to_string());
    }
    if let Some(h) = password_hash {
        map.insert("OSU_PASSWORD_HASH".to_string(), h.to_string());
    }
    if let Some(k) = api_key {
        map.insert("OSU_API_KEY".to_string(), k.to_string());
        std::env::set_var("OSU_API_KEY", k);
    }
    if let Some(d) = daily_key {
        map.insert("OSU_DAILY_API_KEY".to_string(), d.to_string());
        std::env::set_var("OSU_DAILY_API_KEY", d);
    }
    if let Some(cid) = client_id {
        map.insert("OSU_CLIENT_ID".to_string(), cid.to_string());
        std::env::set_var("OSU_CLIENT_ID", cid);
    }
    if let Some(cs) = client_secret {
        map.insert("OSU_CLIENT_SECRET".to_string(), cs.to_string());
        std::env::set_var("OSU_CLIENT_SECRET", cs);
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
    if let Some(recent) = enable_recent_channel {
        map.insert("ENABLE_RECENT_CHANNEL".to_string(), recent.to_string());
    }

    map.entry("SERVER_HOST".to_string()).or_insert_with(|| "127.0.0.1".to_string());
    map.entry("SERVER_PORT".to_string()).or_insert_with(|| "5000".to_string());

    let mut out = String::from("# osu-echo - Environment Configuration\n");
    out.push_str(&format!("SERVER_HOST={}\n", map.get("SERVER_HOST").unwrap()));
    out.push_str(&format!("SERVER_PORT={}\n\n", map.get("SERVER_PORT").unwrap()));

    out.push_str("# Player Profile Settings\n");
    out.push_str(&format!("COUNTRY={}\n\n", map.get("COUNTRY").cloned().unwrap_or_default()));

    out.push_str("# Essential API Keys\n");
    out.push_str("# (Stored strictly locally in this file; never sent to or shared with any remote server)\n");
    out.push_str("# Legacy v1 API key for online beatmap leaderboards: https://old.ppy.sh/p/api/\n");
    out.push_str("# (Scroll down to 'Legacy API' -> 'New Legacy API Key' -> Name: localhost, URL: https://osu-echo.local)\n");
    out.push_str(&format!("OSU_API_KEY={}\n", map.get("OSU_API_KEY").cloned().unwrap_or_default()));
    out.push_str("# osudaily API key for real-time global rank calculation: https://osudaily.net/api.php\n");
    out.push_str(&format!("OSU_DAILY_API_KEY={}\n\n", map.get("OSU_DAILY_API_KEY").cloned().unwrap_or_default()));

    out.push_str("# Official osu! API OAuth application credentials for country rankings (https://osu.ppy.sh/home/account/edit#oauth)\n");
    out.push_str(&format!("OSU_CLIENT_ID={}\n", map.get("OSU_CLIENT_ID").cloned().unwrap_or_default()));
    out.push_str(&format!("OSU_CLIENT_SECRET={}\n\n", map.get("OSU_CLIENT_SECRET").cloned().unwrap_or_default()));

    out.push_str("# Optional osu! Account Credentials (only needed for '!friend sync')\n");
    out.push_str("# (Stored strictly locally; password is MD5 hashed and never stored in plaintext)\n");
    out.push_str(&format!("OSU_USERNAME={}\n", map.get("OSU_USERNAME").cloned().unwrap_or_default()));
    out.push_str(&format!("OSU_PASSWORD_HASH={}\n\n", map.get("OSU_PASSWORD_HASH").cloned().unwrap_or_default()));

    out.push_str("# Leaderboard Settings\n");
    out.push_str("# Show PP instead of Score on beatmap leaderboards\n");
    out.push_str(&format!("PP_LEADERBOARD={}\n", map.get("PP_LEADERBOARD").cloned().unwrap_or_else(|| "false".to_string())));
    out.push_str("# Show PP in the personal best score panel\n");
    out.push_str(&format!("SHOW_PP_FOR_PERSONAL_BEST={}\n", map.get("SHOW_PP_FOR_PERSONAL_BEST").cloned().unwrap_or_else(|| "false".to_string())));
    out.push_str("# Number of scores shown on leaderboards (1-100)\n");
    out.push_str(&format!("AMOUNT_OF_SCORES_ON_LB={}\n\n", map.get("AMOUNT_OF_SCORES_ON_LB").cloned().unwrap_or_else(|| "50".to_string())));

    out.push_str("# Chat Feed Settings\n");
    out.push_str(&format!("ENABLE_RECENT_CHANNEL={}\n\n", map.get("ENABLE_RECENT_CHANNEL").cloned().unwrap_or_else(|| "true".to_string())));

    out.push_str("# Discord Webhook Integration\n");
    out.push_str("# (Posts submitted scores directly to a Discord channel via webhook)\n");
    out.push_str(&format!("DISCORD_WEBHOOK_URL={}\n", map.get("DISCORD_WEBHOOK_URL").cloned().unwrap_or_default()));
    out.push_str(&format!("DISCORD_WEBHOOK_MIN_PP={}\n", map.get("DISCORD_WEBHOOK_MIN_PP").cloned().unwrap_or_default()));

    std::fs::write(env_path, out)?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn setup_tls_and_hosts(data_dir: &Path) {
    match crate::server::tls::get_or_create_certificates(data_dir) {
        Ok(paths) => {
            if let Err(e) = crate::server::tls::install_windows_trust(&paths.cert_der) {
                crate::logger::warn(&format!("Failed to install certificate: {}", e));
            }
            if let Err(e) = crate::server::tls::setup_windows_hosts() {
                crate::logger::warn(&format!("Failed to configure hosts file: {}", e));
            }
        }
        Err(e) => {
            crate::logger::warn(&format!("Failed to generate certificate: {}", e));
        }
    }
}

fn print_setup_header(title: &str, subtitle: &str) {
    crate::clear_console();
    println!("\n{}", crate::LOGO.magenta().bold());
    println!("  {}", title.magenta().bold());
    if !subtitle.is_empty() {
        println!("  {}\n", subtitle.dimmed());
    } else {
        println!();
    }
}

fn print_completion_guide(is_advanced: bool, host: &str, port: u16) {
    print_setup_header(
        if is_advanced { "✔ Advanced Setup Complete!" } else { "✔ Quick Setup Complete!" },
        "Configuration saved to .env and database",
    );
    println!("  {}", "▶ HOW TO CONNECT YOUR OSU! CLIENT:".cyan().bold());
    println!("    1. Right-click your osu! shortcut and choose Properties.");
    println!("    2. In the Target field, append to the end:");
    if host == "127.0.0.1" && port == 5000 {
        println!("         {}", "-devserver localhost".yellow().bold());
        println!("       Example: \"C:\\Games\\osu!\\osu!.exe\" -devserver localhost");
    } else {
        println!("         {}", format!("-devserver {}:{}", host, port).yellow().bold());
        println!("       Example: \"C:\\Games\\osu!\\osu!.exe\" -devserver {}:{}", host, port);
    }
    println!("    3. Launch osu! using that shortcut.");
    println!("    4. Log in with {} username and password!", "ANY".bold());
    println!("       (Your account will be created automatically on first login)\n");
}

fn prompt_api_keys(title: &str, subtitle: &str) -> (Option<String>, Option<String>, Option<String>, Option<String>) {
    loop {
        print_setup_header(title, subtitle);

        println!("  {} All keys and credentials are stored strictly locally in your .env file", "Privacy Note:".green().bold());
        println!("  {} and are NEVER sent to or shared with any remote servers.\n", "             ".normal());

        println!("  1. osu! Legacy API Key (v1):");
        println!("     Required for online beatmap leaderboards & global scores comparison.");
        println!("     URL: {}", "https://old.ppy.sh/p/api/".cyan());
        println!("     {}", "Tip: Scroll down to 'Legacy API' -> Click 'New Legacy API Key'.".dimmed());
        println!("     {}", "     For Application Name & URL, enter 'localhost' and 'https://osu-echo.local'".dimmed());
        let osu_key = prompt_optional_string("     API key (or Enter to skip): ");

        println!("\n  2. osudaily API Key:");
        println!("     Required for real-time global rank calculation based on your PP.");
        println!("     URL: {}", "https://osudaily.net/api.php".cyan());
        let daily_key = prompt_optional_string("     API key (or Enter to skip): ");

        println!("\n  3. osu! OAuth Application Credentials (v2 API) [Optional]:");
        println!("     Required for real-time country rank calculation on your user profile.");
        println!("     URL: {}", "https://osu.ppy.sh/home/account/edit#oauth".cyan());
        println!("     {}", "Tip: Scroll down to 'OAuth' -> Click 'New OAuth Application'.".dimmed());
        println!("     {}", "     For Application Name & Callback URL, enter 'osu-echo' and 'http://localhost'".dimmed());
        let client_id = prompt_optional_string("     Client ID (or Enter to skip): ");
        let client_secret = if client_id.is_some() {
            prompt_optional_string("     Client Secret (or Enter to skip): ")
        } else {
            None
        };

        if osu_key.is_none() || daily_key.is_none() {
            println!("\n  {}", "=========================================================================".red().bold());
            println!("  {} {}", "CRITICAL WARNING:".red().bold(), "ESSENTIAL SERVER API KEYS ARE MISSING!".bright_red().bold());
            println!("  {}", "=========================================================================".red().bold());
            println!("  {}", "osu-echo requires these API keys to function as an authentic Bancho server.".white().bold());
            println!("  {}", "Running without them severely degrades core gameplay and online features:".white().bold());
            println!();
            if osu_key.is_none() {
                println!("  {} {}", "• osu! Legacy v1 API Key:".red().bold(), "MISSING".on_red().white().bold());
                println!("    {} Online beatmap leaderboards will NOT load in-game.", "Consequence:".yellow().bold());
                println!("    {} Score comparisons against official Bancho will be completely disabled.", "            ".dimmed());
                println!("    {} In-game beatmap ranking and direct score fetching will fail.", "            ".dimmed());
            }
            if daily_key.is_none() {
                if osu_key.is_none() {
                    println!();
                }
                println!("  {} {}", "• osudaily API Key:".red().bold(), "MISSING".on_red().white().bold());
                println!("    {} Real-time global rank calculation will be disabled.", "Consequence:".yellow().bold());
                println!("    {} Your in-game profile rank will remain stuck as #unranked / #1.", "            ".dimmed());
            }
            println!();
            println!("  {} {}", "NOTE:".cyan().bold(), "Both keys are completely free and take less than 1 minute to get!".yellow());
            println!("  {}", "=========================================================================".red().bold());

            let choice = prompt_input("\n  Configure keys now, or continue anyway? [Enter = configure (recommended), 'y' = continue]: ");
            if !choice.eq_ignore_ascii_case("y") && !choice.eq_ignore_ascii_case("yes") && !choice.eq_ignore_ascii_case("continue") {
                continue;
            }
        }

        return (osu_key, daily_key, client_id, client_secret);
    }
}

fn setup_quick(data_dir: &Path) -> Config {
    let mut config = Config::default();

    print_setup_header("Quick Setup [Step 1/3: osu! Directory]", "Locating your osu! installation folder");
    let detected_osu = detect_osu_path();
    let chosen_path = match detected_osu {
        Some(ref path) => {
            println!("  {} Found osu! at: {}", "✔".green(), path.display().to_string().cyan());
            let use_detected = prompt_bool("  Use this directory? (Y/n) [default: yes]: ", true);
            if use_detected {
                Some(path.to_string_lossy().to_string())
            } else {
                prompt_existing_directory("  Enter osu! folder path (or Enter to skip): ")
            }
        }
        None => {
            println!("  Could not automatically detect your osu! directory.");
            prompt_existing_directory("  Enter osu! folder path (or Enter to skip): ")
        }
    };

    if let Some(p) = chosen_path {
        println!("  {} osu! path set: {}", "✔".green(), p);
        config.paths.osu_path = Some(p);
    } else {
        println!("  {} Skipped osu! path. Path-dependent features will be disabled.", "!".yellow());
    }

    let (osu_key, daily_key, client_id, client_secret) = prompt_api_keys(
        "Quick Setup [Step 2/3: Essential API Keys]",
        "Required for online beatmaps, leaderboards, and rank calculation",
    );
    config.osu_api_key = osu_key;
    config.osu_daily_api_key = daily_key;
    config.osu_client_id = client_id;
    config.osu_client_secret = client_secret;

    #[cfg(target_os = "windows")]
    {
        print_setup_header("Quick Setup [Step 3/3: Local HTTPS Certificate]", "Trust local certificate for -devserver localhost connection");
        println!("  osu! requires a trusted certificate to connect using '-devserver localhost'.");
        let install_cert = prompt_bool("  Install certificate & update hosts file? (Y/n) [default: yes]: ", true);
        if install_cert {
            setup_tls_and_hosts(data_dir);
            println!("\n  {} Certificate installed & hosts file updated successfully.", "✔".green());
        } else {
            println!("  {} Skipped certificate installation. (You can run 'osu-echo --trust-cert' later)", "!".yellow());
        }
        prompt_input("\n  Press Enter to continue...");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = data_dir;
    }

    if let Err(e) = write_dotenv_full(
        Some("127.0.0.1"),
        Some(5000),
        None,
        None,
        None,
        config.osu_api_key.as_deref(),
        config.osu_daily_api_key.as_deref(),
        config.osu_client_id.as_deref(),
        config.osu_client_secret.as_deref(),
        Some(config.pp_leaderboard),
        Some(config.show_pp_for_personal_best),
        Some(config.amount_of_scores_on_lb),
        Some(config.enable_recent_channel),
    ) {
        crate::logger::warn(&format!("Could not write .env file: {}", e));
    } else {
        crate::logger::success("Configuration saved to .env");
    }

    print_completion_guide(false, "127.0.0.1", 5000);
    prompt_input("Press Enter to start osu-echo...");

    config
}

fn setup_advanced(data_dir: &Path) -> Config {
    let mut config = Config::default();

    print_setup_header("Advanced Setup [Step 1/5: Storage & Directories]", "Configure osu! game and data folders");
    let detected_osu = detect_osu_path();
    let prompt = match &detected_osu {
        Some(path) => format!("  osu! folder path [default: {}]: ", path.to_string_lossy()),
        None => "  osu! folder path (or Enter to skip): ".to_string(),
    };

    loop {
        let input = prompt_input(&prompt);
        if input.is_empty() {
            if let Some(ref detected) = detected_osu {
                config.paths.osu_path = Some(detected.to_string_lossy().to_string());
            }
            break;
        }

        let p = PathBuf::from(&input);
        if !p.exists() {
            println!("  {} Directory '{}' does not exist! Please try again.", "✖".red(), input);
            continue;
        }
        config.paths.osu_path = Some(input);
        break;
    }

    if config.paths.osu_path.is_some() {
        let customize = prompt_bool("  Customize Songs / Replays / Screenshots folders? (y/N) [default: no]: ", false);
        if customize {
            config.paths.songs = prompt_optional_path("    • Songs folder path (or Enter for default): ");
            config.paths.replay = prompt_optional_path("    • Replays folder path (or Enter for default): ");
            config.paths.screenshots = prompt_optional_path("    • Screenshots folder path (or Enter for default): ");
        }
    }

    print_setup_header("Advanced Setup [Step 2/5: Server & Network Settings]", "Configure bind address, port, and TLS certificates");
    let host_input = prompt_input("  Server Host IP [default: 127.0.0.1]: ");
    let host = if host_input.is_empty() {
        "127.0.0.1".to_string()
    } else {
        host_input
    };

    let port = prompt_port("  Server Port [default: 5000]: ", 5000);

    #[cfg(target_os = "windows")]
    {
        let install_cert = prompt_bool("  Install & trust local certificate for osu! client? (Y/n) [default: yes]: ", true);
        if install_cert {
            setup_tls_and_hosts(data_dir);
            println!("\n  {} Certificate installed & hosts file updated successfully.", "✔".green());
        } else {
            println!("  {} Skipped certificate installation.", "!".yellow());
        }
        prompt_input("\n  Press Enter to continue to API keys...");
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = data_dir;
    }

    let (osu_key, daily_key, client_id, client_secret) = prompt_api_keys(
        "Advanced Setup [Step 3/5: Essential API Keys]",
        "API keys for online beatmaps, leaderboards, and rank calculation",
    );
    config.osu_api_key = osu_key;
    config.osu_daily_api_key = daily_key;
    config.osu_client_id = client_id;
    config.osu_client_secret = client_secret;

    print_setup_header("Advanced Setup [Step 4/5: Gameplay & Scoring Rules]", "Configure leaderboard rules and chat feeds");
    config.pp_leaderboard = prompt_bool("  Show PP instead of raw Score on leaderboards? (y/N) [default: no]: ", false);
    config.show_pp_for_personal_best = prompt_bool("  Show PP in personal best score panel? (y/N) [default: no]: ", false);
    config.amount_of_scores_on_lb = prompt_number("  Number of scores to show on leaderboards (1-100) [default: 50]: ", 50, 1, 100);
    config.enable_recent_channel = prompt_bool("  Enable live score announcements in #recent chat channel? (Y/n) [default: yes]: ", true);

    print_setup_header("Advanced Setup [Step 5/5: Profile & Account Sync]", "Configure player profile flag and official Bancho credentials");
    config.country = prompt_optional_country("  Two-letter country code for profile flag (e.g. US, DE, JP, or Enter to skip): ");

    println!("\n  Official osu! Account Credentials:");
    println!("  In-game search and downloads work via Catboy mirror without credentials.");
    println!("  Credentials are only needed if you want to sync your friends list (!friend sync).");
    println!("  {} Credentials are saved only locally in your .env (password is MD5-hashed,", "Privacy Note:".green().bold());
    println!("  {} never stored in plaintext, and never sent to any third-party server).", "             ".normal());
    let setup_creds = prompt_bool("  Configure official osu! account credentials? (y/N) [default: no]: ", false);
    if setup_creds {
        let username = prompt_optional_string("    osu! username (or Enter to skip): ");
        if let Some(ref u) = username {
            let raw_pw = prompt_password("    osu! password (will be MD5-hashed, or Enter to skip) [Hold Tab to reveal]: ");
            if let Some(pw) = raw_pw {
                let hash = format!("{:x}", md5::Md5::digest(pw.as_bytes()));
                config.osu_password = Some(hash);
                println!("    {} Password hashed successfully (plaintext password is never stored).", "✔".green());
            }
            config.osu_username = Some(u.clone());
        }
    }

    print_setup_header("Advanced Setup [Configuration Summary]", "Review your settings before saving");
    println!("  • osu! Path:        {}", config.paths.osu_path.as_deref().unwrap_or("Not configured"));
    println!("  • Bind Address:     {}:{}", host, port);
    println!("  • Country Flag:     {}", config.country.as_deref().unwrap_or("None"));
    println!("  • Leaderboard Mode: {}", if config.pp_leaderboard { "Performance Points (PP)" } else { "Score" });
    println!("  • Leaderboard Size: {} scores", config.amount_of_scores_on_lb);
    println!("  • #recent Feed:     {}", if config.enable_recent_channel { "Enabled" } else { "Disabled" });
    println!(
        "  • osu! v1 API Key:  {}",
        if config.osu_api_key.is_some() {
            "Configured".green()
        } else {
            "CRITICAL: Missing (online beatmap leaderboards disabled)".red().bold()
        }
    );
    println!(
        "  • osudaily API Key: {}",
        if config.osu_daily_api_key.is_some() {
            "Configured".green()
        } else {
            "CRITICAL: Missing (global rank calculation disabled)".red().bold()
        }
    );
    println!(
        "  • osu! OAuth (v2):  {}",
        if config.osu_client_id.is_some() && config.osu_client_secret.is_some() {
            "Configured".green()
        } else {
            "None (country rank calculation disabled)".normal()
        }
    );
    println!("  • Bancho Account:   {}", if config.osu_username.is_some() { "Configured".green() } else { "None".normal() });

    let save = prompt_bool("\nSave this configuration? (Y/n) [default: yes]: ", true);
    if !save {
        println!("  {} Setup cancelled. Using defaults.\n", "!".yellow());
        return Config::default();
    }

    if let Err(e) = write_dotenv_full(
        Some(&host),
        Some(port),
        config.country.as_deref(),
        config.osu_username.as_deref(),
        config.osu_password.as_deref(),
        config.osu_api_key.as_deref(),
        config.osu_daily_api_key.as_deref(),
        config.osu_client_id.as_deref(),
        config.osu_client_secret.as_deref(),
        Some(config.pp_leaderboard),
        Some(config.show_pp_for_personal_best),
        Some(config.amount_of_scores_on_lb),
        Some(config.enable_recent_channel),
    ) {
        crate::logger::warn(&format!("Could not write .env file: {}", e));
    } else {
        crate::logger::success("Configuration saved to .env");
    }

    print_completion_guide(true, &host, port);
    prompt_input("Press Enter to start osu-echo...");

    config
}

pub fn setup_config(data_dir: &Path) -> Config {
    print_setup_header("osu-echo Configuration Wizard", "Select your preferred setup experience:");
    println!("  {} Quick Setup (Recommended)", "[1]".green().bold());
    println!("      • Fast, streamlined setup with smart auto-detection.");
    println!("      • Configures essential API keys and local HTTPS certificate.");
    println!("      • Recommended defaults applied for all other settings.\n");
    println!("  {} Advanced Setup", "[2]".cyan().bold());
    println!("      • Custom folders (Songs, Replays, Screenshots).");
    println!("      • Server host IP / port network settings.");
    println!("      • Essential API keys for online leaderboards & rank calculation.");
    println!("      • Scoring mode, profile country, and official Bancho account sync.\n");

    let choice = prompt_input("Select setup mode [1-2, default: 1]: ");
    let is_advanced = choice == "2" || choice.eq_ignore_ascii_case("advanced") || choice.eq_ignore_ascii_case("a");

    if is_advanced {
        setup_advanced(data_dir)
    } else {
        setup_quick(data_dir)
    }
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

fn prompt_existing_directory(prompt: &str) -> Option<String> {
    loop {
        let input = prompt_input(prompt);
        if input.is_empty() {
            return None;
        }
        let p = PathBuf::from(&input);
        if !p.exists() {
            println!("  {} Directory '{}' does not exist! Please try again.", "✖".red(), input);
            continue;
        }
        return Some(input);
    }
}

pub fn parse_port_input(input: &str, default_val: u16) -> Option<u16> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Some(default_val);
    }
    match trimmed.parse::<u16>() {
        Ok(port) if port > 0 => Some(port),
        _ => None,
    }
}

fn prompt_port(prompt: &str, default_val: u16) -> u16 {
    loop {
        let input = prompt_input(prompt);
        if let Some(port) = parse_port_input(&input, default_val) {
            return port;
        }
        println!("  {} Invalid port number (must be 1-65535). Please try again.", "✖".red());
    }
}

pub fn parse_number_input(input: &str, default_val: i32, min: i32, max: i32) -> Option<i32> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Some(default_val);
    }
    match trimmed.parse::<i32>() {
        Ok(n) if n >= min && n <= max => Some(n),
        _ => None,
    }
}

fn prompt_number(prompt: &str, default_val: i32, min: i32, max: i32) -> i32 {
    loop {
        let input = prompt_input(prompt);
        if let Some(n) = parse_number_input(&input, default_val, min, max) {
            return n;
        }
        println!("  {} Value must be between {} and {}. Please try again.", "✖".red(), min, max);
    }
}

pub fn parse_country_code(input: &str) -> Option<String> {
    let trimmed = input.trim().to_uppercase();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.len() == 2 && trimmed.chars().all(|c| c.is_ascii_alphabetic()) {
        Some(trimmed)
    } else {
        None
    }
}

fn prompt_optional_country(prompt: &str) -> Option<String> {
    loop {
        let input = prompt_input(prompt);
        if input.is_empty() {
            return None;
        }
        if let Some(code) = parse_country_code(&input) {
            return Some(code);
        }
        println!("  {} Country code must be a 2-letter ISO code (e.g. US, DE, JP). Please try again.", "✖".red());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_helpers() {
        assert_eq!(parse_port_input("", 5000), Some(5000));
        assert_eq!(parse_port_input("8080", 5000), Some(8080));
        assert_eq!(parse_port_input("0", 5000), None);
        assert_eq!(parse_port_input("invalid", 5000), None);

        assert_eq!(parse_number_input("", 50, 1, 100), Some(50));
        assert_eq!(parse_number_input("25", 50, 1, 100), Some(25));
        assert_eq!(parse_number_input("0", 50, 1, 100), None);
        assert_eq!(parse_number_input("150", 50, 1, 100), None);

        assert_eq!(parse_country_code(""), None);
        assert_eq!(parse_country_code("us"), Some("US".to_string()));
        assert_eq!(parse_country_code("DE"), Some("DE".to_string()));
        assert_eq!(parse_country_code("USA"), None);
        assert_eq!(parse_country_code("12"), None);
    }

    #[test]
    fn test_paths_resolution() {
        let mut config = Config::default();
        config.paths.osu_path = Some("C:\\osu".to_string());
        assert_eq!(config.songs_folder(), Some(PathBuf::from("C:\\osu\\Songs")));
        assert_eq!(config.replay_folder(), Some(PathBuf::from("C:\\osu\\Replays")));
        assert_eq!(config.screenshots_folder(), Some(PathBuf::from("C:\\osu\\Screenshots")));

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
        std::env::set_var("OSU_CLIENT_ID", "12345");
        std::env::set_var("OSU_CLIENT_SECRET", "supersecret");
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
        assert_eq!(config.osu_client_id.as_deref(), Some("12345"));
        assert_eq!(config.osu_client_secret.as_deref(), Some("supersecret"));
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
        std::env::remove_var("OSU_CLIENT_ID");
        std::env::remove_var("OSU_CLIENT_SECRET");
        std::env::remove_var("PP_LEADERBOARD");
        std::env::remove_var("SHOW_PP_FOR_PERSONAL_BEST");
        std::env::remove_var("AMOUNT_OF_SCORES_ON_LB");
        std::env::remove_var("ENABLE_RECENT_CHANNEL");
    }
}
