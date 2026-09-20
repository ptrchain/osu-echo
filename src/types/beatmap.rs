use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Beatmap {
    pub beatmapset_id: i64,
    pub beatmap_id: i64,
    pub approved: i32,
    pub total_length: i32,
    pub hit_length: i32,
    pub version: String,
    pub file_md5: String,
    pub diff_size: f64,
    pub diff_overall: f64,
    pub diff_approach: f64,
    pub diff_drain: f64,
    pub mode: i32,
    pub count_normal: i32,
    pub count_slider: i32,
    pub count_spinner: i32,
    pub submit_date: String,
    pub approved_date: Option<String>,
    pub last_update: String,
    pub artist: String,
    pub artist_unicode: Option<String>,
    pub title: String,
    pub title_unicode: Option<String>,
    pub creator: String,
    pub creator_id: i64,
    pub bpm: f64,
    pub source: String,
    pub tags: String,
    pub genre_id: i32,
    pub language_id: i32,
    pub favourite_count: i32,
    pub rating: f64,
    pub storyboard: i32,
    pub video: i32,
    pub download_unavailable: i32,
    pub audio_unavailable: i32,
    pub playcount: i32,
    pub passcount: i32,
    pub max_combo: i32,
    pub diff_aim: f64,
    pub diff_speed: f64,
    pub difficultyrating: f64,
    pub file_content: Option<String>,
}

impl Beatmap {
    pub fn blank() -> Self {
        Self {
            beatmapset_id: 0,
            beatmap_id: 0,
            approved: 0,
            total_length: 0,
            hit_length: 0,
            version: String::new(),
            file_md5: String::new(),
            diff_size: 0.0,
            diff_overall: 0.0,
            diff_approach: 0.0,
            diff_drain: 0.0,
            mode: 0,
            count_normal: 0,
            count_slider: 0,
            count_spinner: 0,
            submit_date: String::new(),
            approved_date: None,
            last_update: String::new(),
            artist: String::new(),
            artist_unicode: None,
            title: String::new(),
            title_unicode: None,
            creator: String::new(),
            creator_id: 0,
            bpm: 0.0,
            source: String::new(),
            tags: String::new(),
            genre_id: 0,
            language_id: 0,
            favourite_count: 0,
            rating: 0.0,
            storyboard: 0,
            video: 0,
            download_unavailable: 0,
            audio_unavailable: 0,
            playcount: 0,
            passcount: 0,
            max_combo: 0,
            diff_aim: 0.0,
            diff_speed: 0.0,
            difficultyrating: 0.0,
            file_content: None,
        }
    }

    pub fn from_api_json(json: &serde_json::Value) -> Option<Self> {
        Some(Self {
            beatmapset_id: parse_i64(json, "beatmapset_id")?,
            beatmap_id: parse_i64(json, "beatmap_id")?,
            approved: parse_i32(json, "approved").unwrap_or(0),
            total_length: parse_i32(json, "total_length").unwrap_or(0),
            hit_length: parse_i32(json, "hit_length").unwrap_or(0),
            version: json.get("version")?.as_str().unwrap_or("").to_string(),
            file_md5: json.get("file_md5")?.as_str().unwrap_or("").to_string(),
            diff_size: parse_f64(json, "diff_size").unwrap_or(0.0),
            diff_overall: parse_f64(json, "diff_overall").unwrap_or(0.0),
            diff_approach: parse_f64(json, "diff_approach").unwrap_or(0.0),
            diff_drain: parse_f64(json, "diff_drain").unwrap_or(0.0),
            mode: parse_i32(json, "mode").unwrap_or(0),
            count_normal: parse_i32(json, "count_normal").unwrap_or(0),
            count_slider: parse_i32(json, "count_slider").unwrap_or(0),
            count_spinner: parse_i32(json, "count_spinner").unwrap_or(0),
            submit_date: json.get("submit_date").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            approved_date: json.get("approved_date").and_then(|v| v.as_str()).map(String::from),
            last_update: json.get("last_update").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            artist: json.get("artist").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            artist_unicode: json.get("artist_unicode").and_then(|v| v.as_str()).map(String::from),
            title: json.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            title_unicode: json.get("title_unicode").and_then(|v| v.as_str()).map(String::from),
            creator: json.get("creator").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            creator_id: parse_i64(json, "creator_id").unwrap_or(0),
            bpm: parse_f64(json, "bpm").unwrap_or(0.0),
            source: json.get("source").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            tags: json.get("tags").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            genre_id: parse_i32(json, "genre_id").unwrap_or(0),
            language_id: parse_i32(json, "language_id").unwrap_or(0),
            favourite_count: parse_i32(json, "favourite_count").unwrap_or(0),
            rating: parse_f64(json, "rating").unwrap_or(0.0),
            storyboard: parse_i32(json, "storyboard").unwrap_or(0),
            video: parse_i32(json, "video").unwrap_or(0),
            download_unavailable: parse_i32(json, "download_unavailable").unwrap_or(0),
            audio_unavailable: parse_i32(json, "audio_unavailable").unwrap_or(0),
            playcount: parse_i32(json, "playcount").unwrap_or(0),
            passcount: parse_i32(json, "passcount").unwrap_or(0),
            max_combo: parse_i32(json, "max_combo").unwrap_or(0),
            diff_aim: parse_f64(json, "diff_aim").unwrap_or(0.0),
            diff_speed: parse_f64(json, "diff_speed").unwrap_or(0.0),
            difficultyrating: parse_f64(json, "difficultyrating").unwrap_or(0.0),
            file_content: None,
        })
    }
}

fn parse_i64(json: &serde_json::Value, key: &str) -> Option<i64> {
    let v = json.get(key)?;
    if let Some(n) = v.as_i64() {
        return Some(n);
    }
    v.as_str()?.parse().ok()
}

fn parse_i32(json: &serde_json::Value, key: &str) -> Option<i32> {
    parse_i64(json, key).map(|n| n as i32)
}

fn parse_f64(json: &serde_json::Value, key: &str) -> Option<f64> {
    let v = json.get(key)?;
    if let Some(n) = v.as_f64() {
        return Some(n);
    }
    v.as_str()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_beatmap_blank_defaults() {
        let b = Beatmap::blank();
        assert_eq!(b.approved, 0, "Blank beatmaps must default to unranked (0)");
        assert_eq!(b.beatmap_id, 0);
        assert_eq!(b.beatmapset_id, 0);
    }
}
