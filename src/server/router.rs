use std::collections::HashMap;

pub fn match_route(path: &str, _query: &HashMap<String, String>) -> RouteMatch {
    let cho_prefixes = ["/c4", "/c5", "/c6", "/ce", "/c"];
    for prefix in &cho_prefixes {
        if path.starts_with(prefix) {
            let sub = path.strip_prefix(prefix).unwrap_or("");
            if sub.is_empty() || sub == "/" {
                return RouteMatch::Cho;
            }
        }
    }

    let web_prefixes = ["/osu/web", "/osu"];
    for prefix in &web_prefixes {
        if path.starts_with(prefix) {
            let sub = path.strip_prefix(prefix).unwrap_or(path);
            return RouteMatch::Web(sub.to_string());
        }
    }

    let avatar_sub = if let Some(sub) = path.strip_prefix("/a/") {
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

    if path.starts_with("/d/") {
        let setid_raw = path.strip_prefix("/d/").unwrap_or("0");
        let digits: String = setid_raw.chars().take_while(|c| c.is_ascii_digit()).collect();
        let setid: i64 = digits.parse().unwrap_or(0);
        return RouteMatch::Download(setid);
    }

    if path.starts_with("/beatmaps/") || path.starts_with("/beatmapsets/") {
        return RouteMatch::BeatmapWeb(path.to_string());
    }

    if path.starts_with("/u/") || path.starts_with("/users/") {
        let sub = if let Some(s) = path.strip_prefix("/u/") { s } else { path.strip_prefix("/users/").unwrap_or("") };
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

    RouteMatch::NotFound
}

#[derive(Debug, PartialEq)]
pub enum RouteMatch {
    Cho,
    Web(String),
    Avatar(i32),
    Api(String),
    Download(i64),
    BeatmapWeb(String),
    Screenshot(String),
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
}
