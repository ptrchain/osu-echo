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

    if path.starts_with("/a/") {
        let userid_str = path.strip_prefix("/a/").unwrap_or("0");
        let userid: i32 = userid_str.parse().unwrap_or(0);
        return RouteMatch::Avatar(userid);
    }

    if path.starts_with("/api/v1/") {
        let sub = path.strip_prefix("/api/v1").unwrap_or(path);
        return RouteMatch::Api(sub.to_string());
    }

    if path.starts_with("/d/") {
        let setid_str = path.strip_prefix("/d/").unwrap_or("0");
        let setid: i64 = setid_str.parse().unwrap_or(0);
        return RouteMatch::Download(setid);
    }

    if path.starts_with("/beatmaps/") || path.starts_with("/beatmapsets/") {
        return RouteMatch::BeatmapWeb(path.to_string());
    }

    if path.starts_with("/ss/") {
        let link = path.strip_prefix("/ss/").unwrap_or("");
        return RouteMatch::Screenshot(link.to_string());
    }

    RouteMatch::NotFound
}

#[derive(Debug)]
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
