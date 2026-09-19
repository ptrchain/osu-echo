use std::collections::HashMap;

pub fn match_route(path: &str, _query: &HashMap<String, String>) -> RouteMatch {
    if path.is_empty() || path == "/" {
        return RouteMatch::Status;
    }

    if path == "/favicon.ico" {
        return RouteMatch::Favicon;
    }

    let cho_prefixes = ["/c4", "/c5", "/c6", "/ce", "/c"];
    for prefix in &cho_prefixes {
        if path.starts_with(prefix) {
            let sub = path.strip_prefix(prefix).unwrap_or("");
            if sub.is_empty() || sub == "/" {
                return RouteMatch::Cho;
            }
        }
    }

    // Direct beatmap download: /d/123, /d/123n, /osu/d/123, /b/d/123
    let download_sub = if let Some(sub) = path.strip_prefix("/d/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/osu/d/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/b/d/") {
        Some(sub)
    } else {
        None
    };

    if let Some(sub) = download_sub {
        let digits: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
        let setid: i64 = digits.parse().unwrap_or(0);
        let no_video = sub.ends_with('n') || sub.contains("novideo");
        return RouteMatch::Download(setid, no_video);
    }

    // Thumbnails: /thumb/123l.jpg, /osu/thumb/123l.jpg, /b/thumb/123l.jpg, /b/123l.jpg
    let thumb_sub = if let Some(sub) = path.strip_prefix("/thumb/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/osu/thumb/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/b/thumb/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/b/") {
        if sub.ends_with(".jpg") || sub.ends_with(".png") || sub.ends_with(".jpeg") {
            Some(sub)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(filename) = thumb_sub {
        return RouteMatch::Thumbnail(filename.to_string());
    }

    // Previews: /preview/123.mp3, /osu/preview/123.mp3, /b/preview/123.mp3
    let preview_sub = if let Some(sub) = path.strip_prefix("/preview/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/osu/preview/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/b/preview/") {
        Some(sub)
    } else {
        None
    };

    if let Some(filename) = preview_sub {
        return RouteMatch::Preview(filename.to_string());
    }

    // Beatmap cover assets: /assets/..., /beatmaps/.../covers/...
    let asset_sub = if let Some(sub) = path.strip_prefix("/assets/") {
        Some(sub)
    } else if (path.starts_with("/beatmaps/") || path.starts_with("/beatmapsets/"))
        && (path.contains("/covers/") || path.ends_with(".jpg") || path.ends_with(".png"))
    {
        Some(path.trim_start_matches('/'))
    } else if (path.starts_with("/osu/beatmaps/") || path.starts_with("/osu/beatmapsets/"))
        && (path.contains("/covers/") || path.ends_with(".jpg") || path.ends_with(".png"))
    {
        Some(path.strip_prefix("/osu/").unwrap_or(path))
    } else {
        None
    };

    if let Some(sub) = asset_sub {
        return RouteMatch::Asset(sub.to_string());
    }

    let avatar_sub = if let Some(sub) = path.strip_prefix("/a/") {
        Some(sub)
    } else if let Some(sub) = path.strip_prefix("/osu/a/") {
        Some(sub)
    } else if path.starts_with('/') && path.len() > 1 && path[1..].chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        let rest = &path[1..];
        let digits_len = rest.chars().take_while(|c| c.is_ascii_digit()).count();
        if digits_len > 0 {
            let remaining = &rest[digits_len..];
            if remaining.is_empty() || remaining.starts_with(".png") || remaining.starts_with(".jpg") || remaining.starts_with('?') {
                Some(rest)
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    if let Some(sub) = avatar_sub {
        let digits: String = sub.chars().take_while(|c| c.is_ascii_digit()).collect();
        if let Ok(userid) = digits.parse::<i32>() {
            return RouteMatch::Avatar(userid);
        }
    }

    if path.starts_with("/api/v1/") {
        let sub = path.strip_prefix("/api/v1").unwrap_or(path);
        return RouteMatch::Api(sub.to_string());
    }

    if path.starts_with("/beatmaps/") || path.starts_with("/beatmapsets/") {
        return RouteMatch::BeatmapWeb(path.to_string());
    }
    if let Some(sub) = path.strip_prefix("/osu/beatmaps/") {
        return RouteMatch::BeatmapWeb(format!("/beatmaps/{}", sub));
    }
    if let Some(sub) = path.strip_prefix("/osu/beatmapsets/") {
        return RouteMatch::BeatmapWeb(format!("/beatmapsets/{}", sub));
    }

    let user_path = if let Some(sub) = path.strip_prefix("/osu/u/") {
        Some(format!("/u/{}", sub))
    } else if let Some(sub) = path.strip_prefix("/osu/users/") {
        Some(format!("/users/{}", sub))
    } else if path.starts_with("/u/") || path.starts_with("/users/") {
        Some(path.to_string())
    } else {
        None
    };

    if let Some(ref upath) = user_path {
        let sub = if let Some(s) = upath.strip_prefix("/u/") { s } else { upath.strip_prefix("/users/").unwrap_or("") };
        let uid = sub.split('/').next().unwrap_or("").parse::<i32>().unwrap_or(0);
        if uid == 4 {
            return RouteMatch::BeatmapWeb("/users/2070907".to_string());
        } else if uid == 3 {
            return RouteMatch::BeatmapWeb("/users/3".to_string());
        } else if uid > 0 {
            return RouteMatch::BeatmapWeb(format!("/users/{}", uid));
        }
    }

    if path.starts_with("/ss/") {
        let link = path.strip_prefix("/ss/").unwrap_or("");
        return RouteMatch::Screenshot(link.to_string());
    }
    if let Some(sub) = path.strip_prefix("/osu/ss/") {
        return RouteMatch::Screenshot(sub.to_string());
    }

    let web_prefixes = ["/osu/web", "/web", "/osu"];
    for prefix in &web_prefixes {
        if path.starts_with(prefix) {
            let sub = path.strip_prefix(prefix).unwrap_or(path);
            return RouteMatch::Web(sub.to_string());
        }
    }

    RouteMatch::NotFound
}

#[derive(Debug, PartialEq)]
pub enum RouteMatch {
    Status,
    Favicon,
    Cho,
    Web(String),
    Avatar(i32),
    Api(String),
    Download(i64, bool),
    BeatmapWeb(String),
    Screenshot(String),
    Thumbnail(String),
    Preview(String),
    Asset(String),
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avatar_route_matching() {
        let query = HashMap::new();
        assert_eq!(match_route("/a/4", &query), RouteMatch::Avatar(4));
        assert_eq!(match_route("/4", &query), RouteMatch::Avatar(4));
        assert_eq!(match_route("/4.png", &query), RouteMatch::Avatar(4));
        assert_eq!(match_route("/3", &query), RouteMatch::Avatar(3));
        assert_eq!(match_route("/2070907", &query), RouteMatch::Avatar(2070907));
        assert_eq!(match_route("/u/4", &query), RouteMatch::BeatmapWeb("/users/2070907".to_string()));
    }

    #[test]
    fn test_download_routes() {
        let query = HashMap::new();
        assert_eq!(match_route("/d/2620610", &query), RouteMatch::Download(2620610, false));
        assert_eq!(match_route("/osu/d/2620610", &query), RouteMatch::Download(2620610, false));
        assert_eq!(match_route("/d/2620610n", &query), RouteMatch::Download(2620610, true));
        assert_eq!(match_route("/osu/d/2620610n", &query), RouteMatch::Download(2620610, true));
    }

    #[test]
    fn test_thumbnail_and_preview_routes() {
        let query = HashMap::new();
        assert_eq!(match_route("/thumb/2620610l.jpg", &query), RouteMatch::Thumbnail("2620610l.jpg".to_string()));
        assert_eq!(match_route("/osu/thumb/2620610l.jpg", &query), RouteMatch::Thumbnail("2620610l.jpg".to_string()));
        assert_eq!(match_route("/b/thumb/2620610l.jpg", &query), RouteMatch::Thumbnail("2620610l.jpg".to_string()));
        assert_eq!(match_route("/b/2620610l.jpg", &query), RouteMatch::Thumbnail("2620610l.jpg".to_string()));
        assert_eq!(match_route("/preview/2620610.mp3", &query), RouteMatch::Preview("2620610.mp3".to_string()));
        assert_eq!(match_route("/osu/preview/2620610.mp3", &query), RouteMatch::Preview("2620610.mp3".to_string()));
    }

    #[test]
    fn test_web_routes() {
        let query = HashMap::new();
        assert_eq!(match_route("/osu/web/osu-search.php", &query), RouteMatch::Web("/osu-search.php".to_string()));
        assert_eq!(match_route("/web/osu-search.php", &query), RouteMatch::Web("/osu-search.php".to_string()));
        assert_eq!(match_route("/web/osu-osz2-getscores.php", &query), RouteMatch::Web("/osu-osz2-getscores.php".to_string()));
    }
}
