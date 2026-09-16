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
                (300.0 * (ngeki + n300) as f64 + 200.0 * nkatu as f64 + 100.0 * n100 as f64 + 50.0 * n50 as f64)
                    / (300.0 * total)
                    * 100.0
            } else {
                0.0
            }
        }
        _ => calculate_accuracy(0, n300, n100, n50, ngeki, nkatu, nmiss),
    }
}

#[allow(clippy::too_many_arguments)]
pub fn get_grade(
    mode: u8,
    n300: i32,
    n100: i32,
    n50: i32,
    _ngeki: i32,
    _nkatu: i32,
    nmiss: i32,
    mods: u32,
    acc: f64,
) -> &'static str {
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

pub async fn get_or_fetch_beatmap_content(
    http: &reqwest::Client,
    db: &tokio::sync::Mutex<rusqlite::Connection>,
    config: &crate::types::config::Config,
    bmap: &crate::types::beatmap::Beatmap,
) -> Option<String> {
    if let Some(ref content) = bmap.file_content {
        return Some(content.clone());
    }

    if let Some(songs_dir) = config.songs_folder() {
        if let Some(local_c) = find_local_osu_file(&songs_dir, bmap.beatmap_id, bmap.beatmapset_id) {
            let conn = db.lock().await;
            let _ = crate::db::update_beatmap_file_content(&conn, &bmap.file_md5, &local_c);
            return Some(local_c);
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

    let scores: Vec<crate::types::score::BanchoScore> = arr
        .iter()
        .filter_map(crate::types::score::BanchoScore::from_json)
        .collect();

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
        // Standard (mode 0)
        let acc_std = calculate_accuracy(0, 300, 0, 0, 0, 0, 0);
        assert!((acc_std - 100.0).abs() < 1e-6);

        let acc_std_half = calculate_accuracy(0, 0, 100, 0, 0, 0, 0);
        assert!((acc_std_half - (100.0 / 3.0)).abs() < 1e-4);

        // Taiko (mode 1): 300 = 100%, 100 = 50%
        let acc_taiko = calculate_accuracy(1, 100, 100, 0, 0, 0, 0);
        assert!((acc_taiko - 75.0).abs() < 1e-6);

        // Catch (mode 2): (n300 + n100 + n50) / (n300 + n100 + n50 + nmiss + nkatu)
        let acc_catch = calculate_accuracy(2, 100, 50, 50, 0, 0, 0);
        assert!((acc_catch - 100.0).abs() < 1e-6);

        let acc_catch_miss = calculate_accuracy(2, 90, 0, 0, 0, 5, 5);
        assert!((acc_catch_miss - 90.0).abs() < 1e-6);

        // Mania (mode 3): (300*(ngeki+300) + 200*katu + 100*100 + 50*50) / (300 * total)
        let acc_mania_max = calculate_accuracy(3, 100, 0, 0, 100, 0, 0);
        assert!((acc_mania_max - 100.0).abs() < 1e-6);

        let acc_mania_katu = calculate_accuracy(3, 0, 0, 0, 0, 300, 0);
        assert!((acc_mania_katu - (200.0 / 3.0)).abs() < 1e-4);
    }

    #[test]
    fn test_grade_calculation_modes() {
        // Standard SS & SSH
        assert_eq!(get_grade(0, 300, 0, 0, 0, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(0, 300, 0, 0, 0, 0, 0, 8, 100.0), "SSH"); // Hidden

        // Taiko SS & S
        assert_eq!(get_grade(1, 100, 0, 0, 0, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(1, 95, 5, 0, 0, 0, 0, 0, 97.5), "S");

        // Catch S & A
        assert_eq!(get_grade(2, 100, 0, 0, 0, 0, 0, 0, 100.0), "SS");
        assert_eq!(get_grade(2, 99, 0, 0, 0, 0, 1, 0, 99.0), "S");
        assert_eq!(get_grade(2, 96, 0, 0, 0, 0, 4, 0, 96.0), "A");

        // Mania SS & A
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
}
