use hyper::StatusCode;
use rust_embed::RustEmbed;
use std::path::PathBuf;
use crate::server::response::Response;

#[derive(RustEmbed)]
#[folder = "website/static/"]
pub struct EmbeddedAssets;

pub fn clean_path(path: &str) -> String {
    let path = path.trim_start_matches('/').replace('\\', "/");
    let mut parts = Vec::new();
    for seg in path.split('/') {
        if seg.is_empty() || seg == "." {
            continue;
        }
        if seg == ".." {
            return String::new(); // Block directory traversal attempts
        }
        parts.push(seg);
    }
    parts.join("/")
}

pub fn get_asset(path: &str) -> Option<(Vec<u8>, String)> {
    let clean = clean_path(path);
    if clean.is_empty() {
        return None;
    }

    // 1. Check disk override at ./website/static/<clean>
    let local_path = PathBuf::from("website/static").join(&clean);
    if let Ok(canon_base) = std::fs::canonicalize("website/static") {
        if let Ok(canon_file) = std::fs::canonicalize(&local_path) {
            if canon_file.starts_with(&canon_base) && canon_file.is_file() {
                if let Ok(data) = std::fs::read(&canon_file) {
                    let mime = mime_guess::from_path(&clean).first_or_octet_stream().to_string();
                    return Some((data, mime));
                }
            }
        }
    }

    // 2. Fall back to embedded binary assets
    if let Some(file) = EmbeddedAssets::get(&clean) {
        let mime = mime_guess::from_path(&clean).first_or_octet_stream().to_string();
        return Some((file.data.into_owned(), mime));
    }

    None
}

pub fn serve_asset(path: &str) -> Response {
    let clean = clean_path(path);
    if let Some((data, mime)) = get_asset(&clean) {
        let is_html = clean == "index.html" || mime.starts_with("text/html");
        let mut resp = Response::new(data)
            .with_header("Content-Type", if is_html { "text/html; charset=utf-8" } else { &mime })
            .with_header("X-Content-Type-Options", "nosniff");

        if is_html {
            resp = resp
                .with_header("Cache-Control", "no-cache")
                .with_header(
                    "Content-Security-Policy",
                    "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' https: data:; font-src 'self' https://osu.ppy.sh; connect-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'",
                );
        } else {
            resp = resp.with_header("Cache-Control", "public, max-age=86400");
        }
        resp
    } else {
        Response::empty().with_status(StatusCode::NOT_FOUND)
    }
}

pub fn serve_index() -> Response {
    serve_asset("index.html")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_path_security() {
        assert_eq!(clean_path("index.html"), "index.html");
        assert_eq!(clean_path("/vendor/osu-web.css"), "vendor/osu-web.css");
        assert_eq!(clean_path("vendor\\osu-web.css"), "vendor/osu-web.css");
        assert_eq!(clean_path("../Cargo.toml"), "");
        assert_eq!(clean_path("vendor/../../Cargo.toml"), "");
        assert_eq!(clean_path(""), "");
    }

    #[test]
    fn test_embedded_assets_exist() {
        assert!(EmbeddedAssets::get("index.html").is_some());
        assert!(EmbeddedAssets::get("profile.css").is_some());
        assert!(EmbeddedAssets::get("profile.js").is_some());
        assert!(EmbeddedAssets::get("vendor/osu-web.css").is_some());
        assert!(EmbeddedAssets::get("vendor/avatar-guest@2x.01495bc4.png").is_some());
    }

    #[test]
    fn test_serve_asset_response() {
        let resp = serve_index();
        assert_eq!(resp.status, StatusCode::OK);
        assert!(resp.headers.iter().any(|(k, v)| k == "Content-Type" && v.contains("text/html")));
        assert!(resp.headers.iter().any(|(k, v)| k == "X-Content-Type-Options" && v == "nosniff"));

        let not_found = serve_asset("nonexistent_file.xyz");
        assert_eq!(not_found.status, StatusCode::NOT_FOUND);

        let traversal = serve_asset("../Cargo.toml");
        assert_eq!(traversal.status, StatusCode::NOT_FOUND);
    }
}
