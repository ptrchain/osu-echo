use colored::Colorize;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Info,
    Success,
    Warn,
    Error,
    Debug,
}

fn timestamp() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn log(level: LogLevel, message: &str) {
    let ts = timestamp();
    let prefix = match level {
        LogLevel::Info => "[INFO]".cyan().bold(),
        LogLevel::Success => "[OK]".green().bold(),
        LogLevel::Warn => "[WARN]".yellow().bold(),
        LogLevel::Error => "[ERROR]".red().bold(),
        LogLevel::Debug => "[DEBUG]".magenta().bold(),
    };

    println!("{} {} {}", ts.dimmed(), prefix, message);
}

pub fn info(message: &str) {
    log(LogLevel::Info, message);
}

pub fn success(message: &str) {
    log(LogLevel::Success, message);
}

pub fn warn(message: &str) {
    log(LogLevel::Warn, message);
}

pub fn error(message: &str) {
    log(LogLevel::Error, message);
}

pub fn debug(message: &str) {
    log(LogLevel::Debug, message);
}

pub fn http_request(method: &str, path: &str, status: u16, duration: Duration) {
    // Suppress high-frequency Bancho heartbeat polling in console
    let clean_path = path.split('?').next().unwrap_or(path);
    if (clean_path == "/" || clean_path == "/c" || clean_path == "/c/") && status == 200 {
        return;
    }

    let ts = timestamp();
    let prefix = "[HTTP]".blue().bold();
    let status_styled = if status < 300 {
        format!("{}", status).green()
    } else if status < 400 {
        format!("{}", status).cyan()
    } else if status < 500 {
        format!("{}", status).yellow()
    } else {
        format!("{}", status).red().bold()
    };

    let ms = duration.as_millis();
    let dur_styled = if ms < 50 {
        format!("{}ms", ms).green()
    } else if ms < 200 {
        format!("{}ms", ms).yellow()
    } else {
        format!("{}ms", ms).red()
    };

    println!("{} {} {} {} -> {} ({})", ts.dimmed(), prefix, method.bold(), path, status_styled, dur_styled);
}
