pub fn log(message: &str) {
    crate::logger::info(message);
}

pub fn log_success(message: &str) {
    crate::logger::success(message);
}

pub fn log_error(message: &str) {
    crate::logger::error(message);
}

pub fn get_mode_name(mode: u8) -> &'static str {
    match mode {
        0 => "osu!",
        1 => "osu!taiko",
        2 => "osu!catch",
        3 => "osu!mania",
        _ => "osu!",
    }
}

#[allow(clippy::too_many_arguments)]
pub fn calculate_accuracy(mode: u8, n300: i32, n100: i32, n50: i32, ngeki: i32, nkatu: i32, nmiss: i32) -> f64 {
    match mode {
        0 => {
            let total = (n300 + n100 + n50 + nmiss) as f64;
            if total > 0.0 {
                (n300 as f64 * 300.0 + n100 as f64 * 100.0 + n50 as f64 * 50.0) / (total * 300.0) * 100.0
            } else {
                0.0
            }
        }
        1 => {
            let total = (n300 + n100 + nmiss) as f64;
            if total > 0.0 {
                (n300 as f64 + n100 as f64 * 0.5) / total * 100.0
            } else {
                0.0
            }
        }
        2 => {
            let total = (n300 + n100 + n50 + nmiss + nkatu) as f64;
            if total > 0.0 {
                (n300 + n100 + n50) as f64 / total * 100.0
            } else {
                0.0
            }
        }
        3 => {
            let total = (ngeki + n300 + nkatu + n100 + n50 + nmiss) as f64;
            if total > 0.0 {
                (300.0 * (ngeki + n300) as f64 + 200.0 * nkatu as f64 + 100.0 * n100 as f64 + 50.0 * n50 as f64) / (300.0 * total) * 100.0
            } else {
                0.0
            }
        }
        _ => calculate_accuracy(0, n300, n100, n50, ngeki, nkatu, nmiss),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn get_grade(mode: u8, n300: i32, n100: i32, n50: i32, _ngeki: i32, _nkatu: i32, nmiss: i32, mods: u32, acc: f64) -> &'static str {
    let using_hdfl = mods & 1032 != 0;

    match mode {
        0 => {
            let total = n300 + n100 + n50 + nmiss;
            if total == 0 {
                return "D";
            }
            if n300 == total {
                return if using_hdfl { "SSH" } else { "SS" };
            }

            let n300_percent = n300 as f64 / total as f64;
            let nomiss = nmiss == 0;

            if n300_percent > 0.9 {
                if nomiss && (n50 as f64 / total as f64) < 0.01 {
                    return if using_hdfl { "SH" } else { "S" };
                } else {
                    return "A";
                }
            }

            if n300_percent > 0.8 {
                return if nomiss { "A" } else { "B" };
            }

            if n300_percent > 0.7 {
                return if nomiss { "B" } else { "C" };
            }

            if n300_percent > 0.6 {
                return "C";
            }

            "D"
        }
        1 => {
            let total = n300 + n100 + nmiss;
            if total == 0 {
                return "D";
            }
            if nmiss == 0 && n100 == 0 {
                return if using_hdfl { "SSH" } else { "SS" };
            }
            let n300_ratio = n300 as f64 / total as f64;
            let nomiss = nmiss == 0;

            if nomiss && n300_ratio > 0.9 {
                return if using_hdfl { "SH" } else { "S" };
            }
            if (nomiss && n300_ratio > 0.8) || n300_ratio > 0.9 {
                return "A";
            }
            if (nomiss && n300_ratio > 0.7) || n300_ratio > 0.8 {
                return "B";
            }
            if n300_ratio > 0.6 {
                return "C";
            }
            "D"
        }
        2 => {
            if acc >= 100.0 {
                return if using_hdfl { "SSH" } else { "SS" };
            }
            if acc > 98.0 {
                return if using_hdfl { "SH" } else { "S" };
            }
            if acc > 94.0 {
                return "A";
            }
            if acc > 90.0 {
                return "B";
            }
            if acc > 85.0 {
                return "C";
            }
            "D"
        }
        3 => {
            if acc >= 100.0 {
                return if using_hdfl { "SSH" } else { "SS" };
            }
            if acc > 95.0 {
                return if using_hdfl { "SH" } else { "S" };
            }
            if acc > 90.0 {
                return "A";
            }
            if acc > 80.0 {
                return "B";
            }
            if acc > 70.0 {
                return "C";
            }
            "D"
        }
        _ => get_grade(0, n300, n100, n50, _ngeki, _nkatu, nmiss, mods, acc),
    }
}

pub fn bytes_to_string(b: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(b)
}

pub fn string_to_bytes(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.decode(s)
}

pub fn url_decode(s: &str) -> String {
    percent_encoding::percent_decode_str(s).decode_utf8_lossy().into_owned()
}

pub fn url_encode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

pub fn parse_query_string(query: &str) -> std::collections::HashMap<String, String> {
    url::form_urlencoded::parse(query.as_bytes()).into_owned().collect()
}

pub fn real_type(value: &str) -> serde_json::Value {
    if let Ok(i) = value.parse::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = value.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    serde_json::Value::String(value.to_string())
}

pub fn is_path(p: &str) -> Option<std::path::PathBuf> {
    let path = std::path::PathBuf::from(p);
    if path.is_file() {
        Some(path)
    } else {
        None
    }
}

pub async fn fetch_beatmap_from_api(http: &reqwest::Client, api_key: &str, params: &[(&str, String)]) -> Option<crate::types::beatmap::Beatmap> {
    fetch_beatmap_from_api_with_hint(http, api_key, params, None).await
}

pub async fn fetch_beatmap_from_api_with_hint(
    http: &reqwest::Client,
    api_key: &str,
    params: &[(&str, String)],
    hint: Option<&str>,
) -> Option<crate::types::beatmap::Beatmap> {
    let mut url = reqwest::Url::parse("https://osu.ppy.sh/api/get_beatmaps").ok()?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("k", api_key);
        for (k, v) in params {
            pairs.append_pair(k, v);
        }
    }

    let resp = http.get(url).send().await.ok()?;
    if resp.status() != 200 {
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let arr = json.as_array()?;
    if arr.is_empty() {
        return None;
    }

    if let Some(h) = hint {
        let clean = if let Some(start) = h.rfind('[') {
            let rest = &h[start + 1..];
            let end = rest.find(']').unwrap_or(rest.len());
            &rest[..end]
        } else {
            h
        }
        .trim()
        .to_lowercase();

        if !clean.is_empty() {
            for item in arr {
                if let Some(ver) = item.get("version").and_then(|v| v.as_str()) {
                    if ver.to_lowercase() == clean {
                        return crate::types::beatmap::Beatmap::from_api_json(item);
                    }
                }
            }
            for item in arr {
                if let Some(ver) = item.get("version").and_then(|v| v.as_str()) {
                    if ver.to_lowercase().contains(&clean) || clean.contains(&ver.to_lowercase()) {
                        return crate::types::beatmap::Beatmap::from_api_json(item);
                    }
                }
            }
        }
    }

    crate::types::beatmap::Beatmap::from_api_json(&arr[0])
}

pub async fn fetch_osu_file(http: &reqwest::Client, beatmap_id: i64) -> Option<String> {
    let url = format!("https://osu.ppy.sh/osu/{}", beatmap_id);
    let resp = http.get(&url).send().await.ok()?;
    if resp.status() != 200 {
        return None;
    }
    resp.text().await.ok()
}

pub fn find_local_osu_file(songs_dir: &std::path::Path, beatmap_id: i64, beatmapset_id: i64) -> Option<String> {
    if !songs_dir.exists() {
        return None;
    }

    let entries = std::fs::read_dir(songs_dir).ok()?;
    let prefix = format!("{} ", beatmapset_id);

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            let folder_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if folder_name.starts_with(&prefix) || folder_name == beatmapset_id.to_string() {
                if let Some(content) = check_dir_for_beatmap_id(&path, beatmap_id) {
                    return Some(content);
                }
            }
        }
    }

    None
}

fn check_dir_for_beatmap_id(dir: &std::path::Path, beatmap_id: i64) -> Option<String> {
    let target_needle = format!("BeatmapID:{}", beatmap_id);
    let target_needle_spaced = format!("BeatmapID: {}", beatmap_id);

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("osu") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if content.contains(&target_needle) || content.contains(&target_needle_spaced) {
                        return Some(content);
                    }
                }
            }
        }
    }
    None
}

pub fn parse_osu_file_to_beatmap(content: &str, fallback_bmap_id: Option<i64>, fallback_set_id: Option<i64>) -> Option<crate::types::beatmap::Beatmap> {
    use md5::Digest;

    let mut bmap = crate::types::beatmap::Beatmap::blank();
    let mut found_title = false;
    let mut found_version = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if let Some(val) = trimmed.strip_prefix("Title:") {
            bmap.title = val.trim().to_string();
            found_title = true;
        } else if let Some(val) = trimmed.strip_prefix("TitleUnicode:") {
            bmap.title_unicode = Some(val.trim().to_string());
        } else if let Some(val) = trimmed.strip_prefix("Artist:") {
            bmap.artist = val.trim().to_string();
        } else if let Some(val) = trimmed.strip_prefix("ArtistUnicode:") {
            bmap.artist_unicode = Some(val.trim().to_string());
        } else if let Some(val) = trimmed.strip_prefix("Creator:") {
            bmap.creator = val.trim().to_string();
        } else if let Some(val) = trimmed.strip_prefix("Version:") {
            bmap.version = val.trim().to_string();
            found_version = true;
        } else if let Some(val) = trimmed.strip_prefix("BeatmapID:") {
            if let Ok(id) = val.trim().parse::<i64>() {
                if id > 0 {
                    bmap.beatmap_id = id;
                }
            }
        } else if let Some(val) = trimmed.strip_prefix("BeatmapSetID:") {
            if let Ok(id) = val.trim().parse::<i64>() {
                if id > 0 {
                    bmap.beatmapset_id = id;
                }
            }
        } else if let Some(val) = trimmed.strip_prefix("Mode:") {
            if let Ok(m) = val.trim().parse::<i32>() {
                bmap.mode = m;
            }
        }
    }

    if !found_title || !found_version {
        return None;
    }

    if bmap.beatmap_id == 0 {
        bmap.beatmap_id = fallback_bmap_id.unwrap_or(0);
    }
    if bmap.beatmapset_id == 0 {
        bmap.beatmapset_id = fallback_set_id.unwrap_or(0);
    }

    let md5_hash = format!("{:x}", md5::Md5::digest(content.as_bytes()));
    bmap.file_md5 = md5_hash;
    bmap.file_content = Some(content.to_string());

    if let Ok(parsed) = rosu_pp::Beatmap::from_bytes(content.as_bytes()) {
        bmap.diff_approach = parsed.ar as f64;
        bmap.diff_overall = parsed.od as f64;
        bmap.diff_size = parsed.cs as f64;
        bmap.diff_drain = parsed.hp as f64;
        let diff = rosu_pp::Difficulty::new().calculate(&parsed);
        bmap.difficultyrating = diff.stars();
        bmap.max_combo = diff.max_combo() as i32;
    }

    Some(bmap)
}

pub fn find_and_parse_local_osu_file(
    songs_dir: &std::path::Path,
    set_id: Option<i64>,
    map_id: Option<i64>,
    map_md5: Option<&str>,
    title_hint: Option<&str>,
) -> Option<(crate::types::beatmap::Beatmap, String)> {
    if !songs_dir.exists() {
        return None;
    }

    if let Some(sid) = set_id {
        let prefix = format!("{} ", sid);
        if let Ok(entries) = std::fs::read_dir(songs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let folder_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if folder_name.starts_with(&prefix) || folder_name == sid.to_string() {
                        if let Some((b, c)) = scan_dir_for_osu_file(&path, map_id, map_md5, title_hint, Some(sid)) {
                            return Some((b, c));
                        }
                    }
                }
            }
        }
    }

    if let Some(hint) = title_hint {
        let words: Vec<&str> = hint.split(&['-', ' ', '[', ']', '(', ')'][..]).filter(|w| w.len() >= 3).collect();
        if let Ok(entries) = std::fs::read_dir(songs_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    let folder_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_lowercase();
                    let matches_any = words.iter().any(|w| folder_name.contains(&w.to_lowercase()));
                    if matches_any {
                        if let Some((b, c)) = scan_dir_for_osu_file(&path, map_id, map_md5, Some(hint), None) {
                            return Some((b, c));
                        }
                    }
                }
            }
        }
    }

    None
}

pub fn resolve_songs_folder(config: &crate::types::config::Config) -> Option<std::path::PathBuf> {
    if let Some(folder) = config.songs_folder() {
        if folder.exists() {
            return Some(folder);
        }
    }
    let fallback_d = std::path::PathBuf::from("D:/osu!/Songs");
    if fallback_d.exists() {
        return Some(fallback_d);
    }
    if let Some(detected) = crate::types::config::detect_osu_path() {
        let songs = detected.join("Songs");
        if songs.exists() {
            return Some(songs);
        }
    }
    let fallback_c = std::path::PathBuf::from("C:/osu!/Songs");
    if fallback_c.exists() {
        return Some(fallback_c);
    }
    None
}

fn scan_dir_for_osu_file(
    dir: &std::path::Path,
    map_id: Option<i64>,
    map_md5: Option<&str>,
    title_hint: Option<&str>,
    fallback_set_id: Option<i64>,
) -> Option<(crate::types::beatmap::Beatmap, String)> {
    let mut candidate: Option<(crate::types::beatmap::Beatmap, String)> = None;
    let mut version_candidate: Option<(crate::types::beatmap::Beatmap, String)> = None;
    let mut title_candidate: Option<(crate::types::beatmap::Beatmap, String)> = None;

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("osu") {
                if let Ok(content) = std::fs::read_to_string(&path) {
                    if let Some(mut bmap) = parse_osu_file_to_beatmap(&content, None, fallback_set_id) {
                        if let Some(md5) = map_md5 {
                            if !md5.is_empty() && bmap.file_md5.eq_ignore_ascii_case(md5) {
                                if bmap.beatmap_id == 0 {
                                    bmap.beatmap_id = map_id.unwrap_or(0);
                                }
                                return Some((bmap, content));
                            }
                        }
                        if let Some(mid) = map_id {
                            if bmap.beatmap_id == mid && mid > 0 {
                                return Some((bmap, content));
                            }
                        }
                        if let Some(hint) = title_hint {
                            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                            let diff_bracket = format!("[{}]", bmap.version);
                            let hint_lower = hint.to_lowercase();
                            if !bmap.version.is_empty()
                                && (fname.to_lowercase().contains(&diff_bracket.to_lowercase())
                                    || hint_lower.contains(&diff_bracket.to_lowercase())
                                    || hint_lower.contains(&bmap.version.to_lowercase())
                                    || bmap.version.to_lowercase() == hint_lower)
                            {
                                if version_candidate.is_none() {
                                    if bmap.beatmap_id == 0 {
                                        bmap.beatmap_id = map_id.unwrap_or(0);
                                    }
                                    version_candidate = Some((bmap.clone(), content.clone()));
                                }
                            } else if (fname.to_lowercase().contains(&hint_lower)
                                || hint_lower.contains(&bmap.title.to_lowercase())
                                || bmap.title.to_lowercase().contains(&hint_lower))
                                && title_candidate.is_none()
                            {
                                if bmap.beatmap_id == 0 {
                                    bmap.beatmap_id = map_id.unwrap_or(0);
                                }
                                title_candidate = Some((bmap.clone(), content.clone()));
                            }
                        }
                        if candidate.is_none() {
                            if bmap.beatmap_id == 0 {
                                bmap.beatmap_id = map_id.unwrap_or(0);
                            }
                            candidate = Some((bmap, content));
                        }
                    }
                }
            }
        }
    }

    if map_md5.is_some() {
        version_candidate.or(title_candidate)
    } else if fallback_set_id.is_some() {
        version_candidate.or(title_candidate).or(candidate)
    } else {
        version_candidate.or(title_candidate)
    }
}

// Retrieves beatmap file content. Validates that local candidate matches the requested
// difficulty (MD5, ID, or version) to prevent multi-diff mapsets from caching the wrong difficulty.
// Fixes bug #B0001 (multi-diff title collision & incorrect PP). Thanks to kaan for reporting it!
pub async fn get_or_fetch_beatmap_content(
    http: &reqwest::Client,
    db: &tokio::sync::Mutex<rusqlite::Connection>,
    config: &crate::types::config::Config,
    bmap: &crate::types::beatmap::Beatmap,
) -> Option<String> {
    if let Some(ref content) = bmap.file_content {
        return Some(content.clone());
    }

    if let Some(songs_dir) = resolve_songs_folder(config) {
        if let Some(local_c) = find_local_osu_file(&songs_dir, bmap.beatmap_id, bmap.beatmapset_id) {
            let conn = db.lock().await;
            let _ = crate::db::update_beatmap_file_content(&conn, &bmap.file_md5, &local_c);
            return Some(local_c);
        }
        let sid = if bmap.beatmapset_id > 0 { Some(bmap.beatmapset_id) } else { None };
        let mid = if bmap.beatmap_id > 0 { Some(bmap.beatmap_id) } else { None };
        let diff_hint = if !bmap.version.is_empty() { format!("[{}]", bmap.version) } else { bmap.title.clone() };
        if let Some((local_bmap, local_c)) = find_and_parse_local_osu_file(&songs_dir, sid, mid, Some(&bmap.file_md5), Some(&diff_hint)) {
            let md5_match = !bmap.file_md5.is_empty() && local_bmap.file_md5.eq_ignore_ascii_case(&bmap.file_md5);
            let id_match = bmap.beatmap_id > 0 && local_bmap.beatmap_id == bmap.beatmap_id;
            let ver_match = !bmap.version.is_empty() && local_bmap.version.eq_ignore_ascii_case(&bmap.version);

            if md5_match || id_match || ver_match {
                let conn = db.lock().await;
                let _ = crate::db::update_beatmap_file_content(&conn, &bmap.file_md5, &local_c);
                return Some(local_c);
            }
        }
    }

    if let Some(fetched_c) = fetch_osu_file(http, bmap.beatmap_id).await {
        let conn = db.lock().await;
        let _ = crate::db::update_beatmap_file_content(&conn, &bmap.file_md5, &fetched_c);
        return Some(fetched_c);
    }

    None
}

pub async fn fetch_scores_from_bancho(
    http: &reqwest::Client,
    api_key: &str,
    beatmap_id: i64,
    mode: i32,
    mods: Option<u32>,
    limit: i32,
) -> Option<Vec<crate::types::score::BanchoScore>> {
    let mut url = reqwest::Url::parse("https://osu.ppy.sh/api/get_scores").ok()?;
    {
        let mut pairs = url.query_pairs_mut();
        pairs.append_pair("k", api_key);
        pairs.append_pair("b", &beatmap_id.to_string());
        pairs.append_pair("m", &mode.to_string());
        pairs.append_pair("limit", &limit.clamp(1, 100).to_string());
        if let Some(m) = mods {
            pairs.append_pair("mods", &m.to_string());
        }
    }

    let resp = http.get(url).send().await.ok()?;
    if resp.status() != 200 {
        return None;
    }

    let json: serde_json::Value = resp.json().await.ok()?;
    let arr = json.as_array()?;

    let scores: Vec<crate::types::score::BanchoScore> = arr.iter().filter_map(crate::types::score::BanchoScore::from_json).collect();

    Some(scores)
}

pub fn calculate_bancho_score_pp(parsed_map: &rosu_pp::Beatmap, mode: i32, score: &mut crate::types::score::BanchoScore) {
    let game_mode = match mode {
        0 => rosu_pp::model::mode::GameMode::Osu,
        1 => rosu_pp::model::mode::GameMode::Taiko,
        2 => rosu_pp::model::mode::GameMode::Catch,
        3 => rosu_pp::model::mode::GameMode::Mania,
        _ => rosu_pp::model::mode::GameMode::Osu,
    };

    let mods_u32: u32 = score.enabled_mods.parse().unwrap_or(0);
    let n300: u32 = score.count300.parse().unwrap_or(0);
    let n100: u32 = score.count100.parse().unwrap_or(0);
    let n50: u32 = score.count50.parse().unwrap_or(0);
    let nkatu: u32 = score.countkatu.parse().unwrap_or(0);
    let ngeki: u32 = score.countgeki.parse().unwrap_or(0);
    let nmiss: u32 = score.countmiss.parse().unwrap_or(0);
    let combo: u32 = score.maxcombo.parse().unwrap_or(0);

    let result = rosu_pp::Performance::new(parsed_map)
        .mode_or_ignore(game_mode)
        .mods(mods_u32)
        .n300(n300)
        .n100(n100)
        .n50(n50)
        .n_katu(nkatu)
        .n_geki(ngeki)
        .misses(nmiss)
        .combo(combo)
        .calculate();

    score.pp = Some(result.pp());
}

pub async fn get_rank_from_daily(http: &reqwest::Client, api_key: &str, pp: i32, mode: u8) -> Option<i32> {
    let url = format!("https://osudaily.net/api/pp.php?k={}&t=pp&v={}&m={}", api_key, pp, mode);
    let resp = http.get(&url).send().await.ok()?;
    if resp.status() != 200 {
        return Some(1);
    }
    let json: serde_json::Value = resp.json().await.ok()?;

    if json.get("error").is_some() {
        return None;
    }

    json.get("rank").and_then(|v| v.as_i64().or_else(|| v.as_str().and_then(|s| s.parse().ok()))).map(|r| r as i32).or(Some(1))
}

// MaxMind GeoIP legacy country code table required by the osu! client
pub const COUNTRY_CODES: &[&str] = &[
    "--", "AP", "EU", "AD", "AE", "AF", "AG", "AI", "AL", "AM", "CW", "AO", "AQ", "AR", "AS", "AT", "AU", "AW", "AZ", "BA", "BB", "BD", "BE", "BF", "BG", "BH",
    "BI", "BJ", "BM", "BN", "BO", "BR", "BS", "BT", "BV", "BW", "BY", "BZ", "CA", "CC", "CD", "CF", "CG", "CH", "CI", "CK", "CL", "CM", "CN", "CO", "CR", "CU",
    "CV", "CX", "CY", "CZ", "DE", "DJ", "DK", "DM", "DO", "DZ", "EC", "EE", "EG", "EH", "ER", "ES", "ET", "FI", "FJ", "FK", "FM", "FO", "FR", "SX", "GA", "GB",
    "GD", "GE", "GF", "GH", "GI", "GL", "GM", "GN", "GP", "GQ", "GR", "GS", "GT", "GU", "GW", "GY", "HK", "HM", "HN", "HR", "HT", "HU", "ID", "IE", "IL", "IN",
    "IO", "IQ", "IR", "IS", "IT", "JM", "JO", "JP", "KE", "KG", "KH", "KI", "KM", "KN", "KP", "KR", "KW", "KY", "KZ", "LA", "LB", "LC", "LI", "LK", "LR", "LS",
    "LT", "LU", "LV", "LY", "MA", "MC", "MD", "MG", "MH", "MK", "ML", "MM", "MN", "MO", "MP", "MQ", "MR", "MS", "MT", "MU", "MV", "MW", "MX", "MY", "MZ", "NA",
    "NC", "NE", "NF", "NG", "NI", "NL", "NO", "NP", "NR", "NU", "NZ", "OM", "PA", "PE", "PF", "PG", "PH", "PK", "PL", "PM", "PN", "PR", "PS", "PT", "PW", "PY",
    "QA", "RE", "RO", "RU", "RW", "SA", "SB", "SC", "SD", "SE", "SG", "SH", "SI", "SJ", "SK", "SL", "SM", "SN", "SO", "SR", "ST", "SV", "SY", "SZ", "TC", "TD",
    "TF", "TG", "TH", "TJ", "TK", "TM", "TN", "TO", "TL", "TR", "TT", "TV", "TW", "TZ", "UA", "UG", "UM", "US", "UY", "UZ", "VA", "VC", "VE", "VG", "VI", "VN",
    "VU", "WF", "WS", "YE", "YT", "RS", "ZA", "ZM", "ME", "ZW", "A1", "A2", "O1", "AX", "GG", "IM", "JE", "BL", "MF", "BQ", "SS", "O1",
];

pub fn country_code_to_byte(code: &str) -> u8 {
    let trimmed = code.trim();
    if trimmed.is_empty() || trimmed == "--" {
        return 0;
    }
    let target = if trimmed.eq_ignore_ascii_case("UK") { "GB" } else { trimmed };
    COUNTRY_CODES.iter().position(|&c| c.eq_ignore_ascii_case(target)).unwrap_or(0) as u8
}

pub fn country_byte_to_code(byte: u8) -> &'static str {
    match COUNTRY_CODES.get(byte as usize).copied() {
        Some("--") | None => "Unknown",
        Some(code) => code,
    }
}

pub async fn fetch_user_stats_from_api(http: &reqwest::Client, api_key: &str, user_query: &str) -> Option<crate::db::FriendRecord> {
    let trimmed = user_query.trim();
    if trimmed.is_empty() {
        return None;
    }
    let is_numeric = trimmed.chars().all(|c| c.is_ascii_digit());
    let param_type = if is_numeric { "id" } else { "string" };
    let url = format!("https://osu.ppy.sh/api/get_user?k={}&u={}&type={}", api_key, url_encode(trimmed), param_type);

    let resp = http.get(&url).send().await.ok()?;
    if resp.status() != 200 {
        return None;
    }

    let users: Vec<serde_json::Value> = resp.json().await.ok()?;
    let u = users.first()?;

    let friend_id = u.get("user_id").and_then(|v| v.as_str()).and_then(|s| s.parse::<i32>().ok())?;
    let friend_name = u.get("username").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let rank = u.get("pp_rank").and_then(|v| v.as_str()).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);
    let pp = u.get("pp_raw").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).map(|f| f.round() as i32).unwrap_or(0);
    let acc = u.get("accuracy").and_then(|v| v.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
    let country_str = u.get("country").and_then(|v| v.as_str()).unwrap_or("");
    let country = country_code_to_byte(country_str);
    let ranked_score = u.get("ranked_score").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let total_score = u.get("total_score").and_then(|v| v.as_str()).and_then(|s| s.parse::<i64>().ok()).unwrap_or(0);
    let playcount = u.get("playcount").and_then(|v| v.as_str()).and_then(|s| s.parse::<i32>().ok()).unwrap_or(0);

    Some(crate::db::FriendRecord { friend_id, friend_name, rank, pp, acc, country, ranked_score, total_score, playcount })
}

pub async fn fetch_user_scores_from_bancho(
    http: &reqwest::Client,
    api_key: &str,
    beatmap_id: i64,
    user_id: i32,
    mode: i32,
) -> Option<Vec<crate::types::score::BanchoScore>> {
    let url = format!("https://osu.ppy.sh/api/get_scores?k={}&b={}&u={}&m={}&type=id", api_key, beatmap_id, user_id, mode);
    let resp = http.get(&url).send().await.ok()?;
    if resp.status() != 200 {
        return None;
    }
    let json: Vec<serde_json::Value> = resp.json().await.ok()?;
    let mut scores = Vec::new();
    for item in json {
        if let Some(s) = crate::types::score::BanchoScore::from_json(&item) {
            scores.push(s);
        }
    }
    Some(scores)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_names() {
        assert_eq!(get_mode_name(0), "osu!");
        assert_eq!(get_mode_name(1), "osu!taiko");
        assert_eq!(get_mode_name(2), "osu!catch");
        assert_eq!(get_mode_name(3), "osu!mania");
        assert_eq!(get_mode_name(99), "osu!");
    }

    #[test]
    fn test_accuracy_calculation_modes() {
        let acc_std = calculate_accuracy(0, 300, 0, 0, 0, 0, 0);
        assert!((acc_std - 100.0).abs() < 1e-6);

        let acc_std_half = calculate_accuracy(0, 0, 100, 0, 0, 0, 0);
        assert!((acc_std_half - (100.0 / 3.0)).abs() < 1e-4);

        let acc_taiko = calculate_accuracy(1, 100, 100, 0, 0, 0, 0);
        assert!((acc_taiko - 75.0).abs() < 1e-6);

        let acc_catch = calculate_accuracy(2, 100, 50, 50, 0, 0, 0);
        assert!((acc_catch - 100.0).abs() < 1e-6);

        let acc_catch_miss = calculate_accuracy(2, 90, 0, 0, 0, 5, 5);
        assert!((acc_catch_miss - 90.0).abs() < 1e-6);

        let acc_mania_max = calculate_accuracy(3, 100, 0, 0, 100, 0, 0);
        assert!((acc_mania_max - 100.0).abs() < 1e-6);

        let acc_mania_katu = calculate_accuracy(3, 0, 0, 0, 0, 300, 0);
        assert!((acc_mania_katu - (200.0 / 3.0)).abs() < 1e-4);
    }

    #[test]
    fn test_grade_calculation_modes() {
        assert_eq!(get_grade(0, 300, 0, 0, 0, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(0, 300, 0, 0, 0, 0, 0, 8, 100.0), "SSH");

        assert_eq!(get_grade(1, 100, 0, 0, 0, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(1, 95, 5, 0, 0, 0, 0, 0, 97.5), "S");

        assert_eq!(get_grade(2, 100, 0, 0, 0, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(2, 99, 0, 0, 0, 0, 1, 0, 99.0), "S");
        assert_eq!(get_grade(2, 96, 0, 0, 0, 0, 4, 0, 96.0), "A");

        assert_eq!(get_grade(3, 50, 0, 0, 50, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(3, 40, 10, 0, 40, 5, 5, 0, 92.0), "A");
    }

    #[test]
    fn test_calculate_bancho_score_pp() {
        let osu_content = "osu file format v14\n[General]\nMode: 0\n[Difficulty]\nHPDrainRate:5\nCircleSize:5\nOverallDifficulty:5\nApproachRate:5\nSliderMultiplier:1.4\nSliderTickRate:1\n[TimingPoints]\n0,500,4,1,0,100,1,0\n[HitObjects]\n256,192,1000,1,0,0:0:0:0:\n";
        let parsed_map = rosu_pp::Beatmap::from_bytes(osu_content.as_bytes()).unwrap();

        let json = serde_json::json!({
            "score_id": "1",
            "username": "TopPlayer",
            "score": "1000000",
            "maxcombo": "1",
            "count50": "0",
            "count100": "0",
            "count300": "1",
            "countmiss": "0",
            "countkatu": "0",
            "countgeki": "0",
            "perfect": "1",
            "enabled_mods": "0",
            "user_id": "100",
            "date": "2024-01-01 00:00:00",
            "replay_available": "1"
        });

        let mut b_score = crate::types::score::BanchoScore::from_json(&json).unwrap();
        assert!(b_score.pp.is_none());

        calculate_bancho_score_pp(&parsed_map, 0, &mut b_score);
        assert!(b_score.pp.is_some());
        assert!(b_score.pp.unwrap() > 0.0);
    }

    #[test]
    fn test_parse_osu_file_to_beatmap_metadata() {
        let content = "osu file format v14\n\
            [General]\nMode: 0\n\
            [Metadata]\n\
            Title:Sidetracked Day GAMMA\n\
            Artist:VINXIS\n\
            Creator:Seamob\n\
            Version:270\n\
            BeatmapID:2543274\n\
            BeatmapSetID:1222729\n\
            [Difficulty]\n\
            HPDrainRate:6\n\
            CircleSize:4\n\
            OverallDifficulty:9.3\n\
            ApproachRate:9.6\n\
            SliderMultiplier:1.4\n\
            SliderTickRate:1\n\
            [TimingPoints]\n\
            0,500,4,1,0,100,1,0\n\
            [HitObjects]\n\
            256,192,1000,1,0,0:0:0:0:\n";

        let bmap = parse_osu_file_to_beatmap(content, None, None).unwrap();
        assert_eq!(bmap.beatmap_id, 2543274);
        assert_eq!(bmap.beatmapset_id, 1222729);
        assert_eq!(bmap.artist, "VINXIS");
        assert_eq!(bmap.title, "Sidetracked Day GAMMA");
        assert_eq!(bmap.version, "270");
        assert!(!bmap.file_md5.is_empty());
        assert_eq!(bmap.file_content, Some(content.to_string()));
    }

    #[test]
    fn test_scan_dir_does_not_false_match_unrelated_map() {
        let temp_dir = std::env::temp_dir().join(format!("osu_test_songs_{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&temp_dir).unwrap();
        let map_path = temp_dir.join("test.osu");
        let content = "osu file format v3\n\
            [General]\nMode: 0\n\
            [Metadata]\n\
            Title:DISCO PRINCE\n\
            Artist:Kenji Ninuma\n\
            Creator:peppy\n\
            Version:Normal\n\
            [Difficulty]\n\
            HPDrainRate:6\n\
            CircleSize:4\n\
            OverallDifficulty:6\n\
            ApproachRate:6\n\
            [HitObjects]\n\
            256,192,1000,1,0,0:0:0:0:\n";
        std::fs::write(&map_path, content).unwrap();

        // When scanning without matching set_id or title or md5 or map_id, it must NOT return a candidate match
        let res = scan_dir_for_osu_file(&temp_dir, Some(999999), None, None, None);
        assert!(res.is_none(), "Must not return unrelated file as match");

        // When scanning with matching title hint, it should match
        let res_title = scan_dir_for_osu_file(&temp_dir, None, None, Some("DISCO PRINCE"), None);
        assert!(res_title.is_some(), "Must match by title hint");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_country_code_and_url_encoding() {
        assert_eq!(country_code_to_byte("AD"), 3);
        assert_eq!(country_code_to_byte("AU"), 16);
        assert_eq!(country_code_to_byte("de"), 56);
        assert_eq!(country_code_to_byte("DZ"), 61);
        assert_eq!(country_code_to_byte("US"), 225);
        assert_eq!(country_code_to_byte("GB"), 77);
        assert_eq!(country_code_to_byte("UK"), 77);
        assert_eq!(country_code_to_byte("SS"), 254);
        assert_eq!(country_code_to_byte("UNKNOWN"), 0);
        assert_eq!(country_code_to_byte(""), 0);

        assert_eq!(country_byte_to_code(16), "AU");
        assert_eq!(country_byte_to_code(56), "DE");
        assert_eq!(country_byte_to_code(61), "DZ");
        assert_eq!(country_byte_to_code(0), "Unknown");

        assert_eq!(url_encode("mrekk"), "mrekk");
        assert_eq!(url_encode("hello world"), "hello+world");
    }
}
