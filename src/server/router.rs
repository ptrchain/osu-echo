use std::collections::HashMap;

/// Maps the first label of the request's `Host` header onto the internal path prefix that
/// endpoint lives under.
///
/// The osu! client addresses a server through several subdomains of one base domain
/// (`c4.<base>` for bancho, `osu!.<base>`/`osu.<base>` for the web API, `a.` avatars, `b.`
/// osu!direct, `assets.` beatmap covers). The base domain is whatever the user passed to
/// `-devserver`, so it cannot be matched by name: only the *label* is meaningful.
///
/// Returning `None` means the host carries no known label, which is what happens when the
/// request arrives through a reverse proxy/tunnel that rewrites `Host` to the upstream
/// address. Callers fall back to [`normalize_client_path`]'s body heuristic for those.
pub fn host_route_prefix(host: &str) -> Option<&'static str> {
    let label = host.split('.').next().unwrap_or("").to_ascii_lowercase();
    match label.as_str() {
        // The bancho host has been spelled `cho`, `c`, `c1`…`c6` and `ce` over the client's
        // lifetime; which one a build asks for depends on its age.
        "c" | "c1" | "c2" | "c3" | "c4" | "c5" | "c6" | "ce" | "cho" => Some("/c"),
        "osu" | "osu!" => Some("/osu"),
        "a" => Some("/a"),
        "b" => Some("/b"),
        "assets" => Some("/assets"),
        _ => None,
    }
}

/// Rewrites an incoming request path so that [`match_route`] sees the same layout it would see
/// for `-devserver localhost`, whatever base domain the client was pointed at.
///
/// The client sends the same paths on every host (`/web/osu-search.php`, `/d/123`, `/`, ...), so
/// the subdomain is the only thing distinguishing "bancho packet POST" from "beatmap download"
/// and friends. Without this, a server reached through `-devserver <domain>` answers bancho
/// traffic with the status page and in-game requests silently do nothing.
///
/// `is_post_with_body` covers the proxied case: bancho traffic is the only POST to `/` that
/// carries a body (the packet stream), and the login POST is the one without an `osu-token`
/// header, so neither can be recognised by header alone.
pub fn normalize_client_path(host: &str, path: &str, is_post_with_body: bool) -> String {
    let path = if path.is_empty() { "/" } else { path };
    let mut normalized = match host_route_prefix(host) {
        Some(prefix) if path == "/" => prefix.to_string(),
        Some(prefix) if path != prefix && !path.starts_with(&format!("{}/", prefix)) => format!("{}{}", prefix, path),
        _ => path.to_string(),
    };

    if is_post_with_body && normalized == "/" {
        normalized = "/c".to_string();
    }

    normalized
}

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
    fn test_route_matching() {
        let query = HashMap::new();
        let cases = [
            ("/a/4", RouteMatch::Avatar(4)),
            ("/4", RouteMatch::Avatar(4)),
            ("/4.png", RouteMatch::Avatar(4)),
            ("/3", RouteMatch::Avatar(3)),
            ("/2070907", RouteMatch::Avatar(2070907)),
            ("/u/4", RouteMatch::BeatmapWeb("/users/2070907".to_string())),
            ("/d/2620610", RouteMatch::Download(2620610, false)),
            ("/osu/d/2620610", RouteMatch::Download(2620610, false)),
            ("/d/2620610n", RouteMatch::Download(2620610, true)),
            ("/osu/d/2620610n", RouteMatch::Download(2620610, true)),
            ("/thumb/2620610l.jpg", RouteMatch::Thumbnail("2620610l.jpg".to_string())),
            ("/osu/thumb/2620610l.jpg", RouteMatch::Thumbnail("2620610l.jpg".to_string())),
            ("/b/thumb/2620610l.jpg", RouteMatch::Thumbnail("2620610l.jpg".to_string())),
            ("/b/2620610l.jpg", RouteMatch::Thumbnail("2620610l.jpg".to_string())),
            ("/preview/2620610.mp3", RouteMatch::Preview("2620610.mp3".to_string())),
            ("/osu/preview/2620610.mp3", RouteMatch::Preview("2620610.mp3".to_string())),
            ("/osu/web/osu-search.php", RouteMatch::Web("/osu-search.php".to_string())),
            ("/web/osu-search.php", RouteMatch::Web("/osu-search.php".to_string())),
            ("/web/osu-osz2-getscores.php", RouteMatch::Web("/osu-osz2-getscores.php".to_string())),
        ];

        for (path, expected) in cases {
            assert_eq!(match_route(path, &query), expected, "Route failed for {}", path);
        }
    }

    #[test]
    fn test_host_route_prefix_ignores_base_domain() {
        for host in ["c.localhost", "c4.localhost", "c.catboy.click", "c4.Catboy.Click", "c5.ppy.sh", "c6.ppy.sh", "ce.ppy.sh", "cho.localhost", "c1.localhost", "c3.ppy.sh"] {
            assert_eq!(host_route_prefix(host), Some("/c"), "wrong prefix for {}", host);
        }
        assert_eq!(host_route_prefix("osu!.catboy.click"), Some("/osu"));
        assert_eq!(host_route_prefix("osu.ppy.sh"), Some("/osu"));
        assert_eq!(host_route_prefix("a.ppy.sh"), Some("/a"));
        assert_eq!(host_route_prefix("b.ppy.sh"), Some("/b"));
        assert_eq!(host_route_prefix("assets.ppy.sh"), Some("/assets"));

        // Hosts without a known label must not be guessed at.
        assert_eq!(host_route_prefix("localhost"), None);
        assert_eq!(host_route_prefix("catboy.click"), None);
        assert_eq!(host_route_prefix("127.0.0.1"), None);
    }

    #[test]
    fn test_normalize_client_path_for_any_devserver_domain() {
        let query = HashMap::new();

        // Bancho traffic: the client POSTs its packet stream to `/` on a `c*` subdomain. With a
        // domain devserver that used to fall through to the status page, so nothing in the game
        // (login, chat, commands) ever got an answer.
        for host in ["c4.catboy.click", "c.localhost", "c6.ppy.sh", "cho.localhost", "c1.localhost"] {
            let path = normalize_client_path(host, "/", true);
            assert_eq!(path, "/c", "wrong normalized path for {}", host);
            assert_eq!(match_route(&path, &query), RouteMatch::Cho);
        }

        // Web API traffic.
        assert_eq!(normalize_client_path("osu!.catboy.click", "/web/osu-search.php", false), "/osu/web/osu-search.php");
        assert_eq!(normalize_client_path("osu.localhost", "/web/osu-search.php", false), "/osu/web/osu-search.php");
        assert_eq!(normalize_client_path("osu.catboy.click", "/web/bancho_connect.php", true), "/osu/web/bancho_connect.php");

        // osu!direct and avatars.
        assert_eq!(normalize_client_path("b.catboy.click", "/d/2620610", false), "/b/d/2620610");
        assert_eq!(normalize_client_path("b.catboy.click", "/thumb/2620610l.jpg", false), "/b/thumb/2620610l.jpg");
        assert_eq!(normalize_client_path("a.catboy.click", "/4", false), "/a/4");
        assert_eq!(normalize_client_path("assets.catboy.click", "/beatmaps/1/covers/card.jpg", false), "/assets/beatmaps/1/covers/card.jpg");

        // Already-prefixed paths are left alone instead of being prefixed twice.
        assert_eq!(normalize_client_path("c4.catboy.click", "/c/", true), "/c/");
        assert_eq!(normalize_client_path("b.catboy.click", "/b/2620610l.jpg", false), "/b/2620610l.jpg");

        // A proxy that rewrites Host to the upstream address loses the subdomain; the packet
        // stream is still recognisable as a POST to `/` with a body.
        assert_eq!(normalize_client_path("127.0.0.1", "/", true), "/c");
        assert_eq!(normalize_client_path("catboy.click", "/", true), "/c");
        // ...but a plain GET to `/` must still serve the status page.
        assert_eq!(normalize_client_path("catboy.click", "/", false), "/");
        assert_eq!(match_route(&normalize_client_path("catboy.click", "/", false), &query), RouteMatch::Status);
        // And web requests through such a proxy keep working off the path alone.
        assert_eq!(match_route(&normalize_client_path("127.0.0.1", "/web/osu-search.php", false), &query), RouteMatch::Web("/osu-search.php".to_string()));
    }
}
