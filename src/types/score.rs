use crate::types::mods::Mods;
use crate::types::replay::Replay;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Score {
    pub mode: i32,
    pub md5: String,
    pub name: String,
    pub n300: i32,
    pub n100: i32,
    pub n50: i32,
    pub ngeki: i32,
    pub nkatu: i32,
    pub nmiss: i32,
    pub score: i64,
    pub max_combo: i32,
    pub perfect: bool,
    pub mods: u32,
    pub time: i64,
    pub acc: Option<f64>,
    pub pp: Option<f64>,
    pub replay_md5: Option<String>,
    pub scoreid: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_frames: Option<String>,
    pub mods_str: Option<String>,
    pub additional_mods: Option<i32>,
    pub submission_checksum: Option<String>,
    pub submission_identity: Option<String>,
}

impl Score {
    pub fn from_replay(replay: &Replay) -> Self {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64;

        Self {
            mode: replay.mode as i32,
            md5: replay.beatmap_md5.clone(),
            name: replay.player_name.clone(),
            n300: replay.n300 as i32,
            n100: replay.n100 as i32,
            n50: replay.n50 as i32,
            ngeki: replay.ngeki as i32,
            nkatu: replay.nkatu as i32,
            nmiss: replay.nmiss as i32,
            score: replay.total_score as i64,
            max_combo: replay.combo as i32,
            perfect: replay.perfect,
            mods: replay.mods.bits(),
            time: now,
            acc: None,
            pp: None,
            replay_md5: Some(replay.replay_md5.clone()),
            scoreid: None,
            replay_frames: Some(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &replay.raw_frames)),
            mods_str: Some(replay.mods.short_name()),
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        }
    }

    pub fn as_leaderboard_entry(&self, pp_leaderboard: bool, show_pp_pb: bool, mode: Option<Mods>) -> LeaderboardEntry {
        let sid = match (self.scoreid, self.replay_frames.as_ref()) {
            (Some(id), Some(_)) => -id,
            _ => 0,
        };

        let pp_checks = pp_leaderboard || mode.is_some() || show_pp_pb;

        let score_val = if pp_checks { self.pp.unwrap_or(0.0) as i64 } else { self.score };

        LeaderboardEntry {
            score_id: sid,
            username: self.name.clone(),
            score: score_val,
            maxcombo: self.max_combo,
            count50: self.n50,
            count100: self.n100,
            count300: self.n300,
            countmiss: self.nmiss,
            countkatu: self.nkatu,
            countgeki: self.ngeki,
            perfect: if self.perfect { 1 } else { 0 },
            enabled_mods: self.mods,
            user_id: 2,
            time: self.time,
            replay_available: if sid != 0 { 1 } else { 0 },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LeaderboardEntry {
    pub score_id: i64,
    pub username: String,
    pub score: i64,
    pub maxcombo: i32,
    pub count50: i32,
    pub count100: i32,
    pub count300: i32,
    pub countmiss: i32,
    pub countkatu: i32,
    pub countgeki: i32,
    pub perfect: i32,
    pub enabled_mods: u32,
    pub user_id: i32,
    pub time: i64,
    pub replay_available: i32,
}

impl LeaderboardEntry {
    pub fn format(&self, num_on_lb: i32) -> String {
        format!(
            "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.score_id,
            self.username,
            self.score,
            self.maxcombo,
            self.count50,
            self.count100,
            self.count300,
            self.countmiss,
            self.countkatu,
            self.countgeki,
            self.perfect,
            self.enabled_mods,
            self.user_id,
            num_on_lb,
            self.time,
            self.replay_available,
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanchoScore {
    pub score_id: String,
    pub username: String,
    pub score: String,
    pub maxcombo: String,
    pub count50: String,
    pub count100: String,
    pub count300: String,
    pub countmiss: String,
    pub countkatu: String,
    pub countgeki: String,
    pub perfect: String,
    pub enabled_mods: String,
    pub user_id: String,
    pub time: i64,
    pub replay_available: String,
    pub pp: Option<f64>,
}

impl BanchoScore {
    pub fn from_json(json: &serde_json::Value) -> Option<Self> {
        let time = json
            .get("date")
            .and_then(|d| d.as_str())
            .map(chrono_parse_to_epoch)
            .unwrap_or_else(|| SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64);

        let pp = json.get("pp").and_then(|v| if let Some(s) = v.as_str() { s.parse::<f64>().ok() } else { v.as_f64() });

        Some(Self {
            score_id: json_get_str(json, "score_id", "0"),
            username: json_get_str(json, "username", ""),
            score: json_get_str(json, "score", "0"),
            maxcombo: json_get_str(json, "maxcombo", "0"),
            count50: json_get_str(json, "count50", "0"),
            count100: json_get_str(json, "count100", "0"),
            count300: json_get_str(json, "count300", "0"),
            countmiss: json_get_str(json, "countmiss", "0"),
            countkatu: json_get_str(json, "countkatu", "0"),
            countgeki: json_get_str(json, "countgeki", "0"),
            perfect: json_get_str(json, "perfect", "0"),
            enabled_mods: json_get_str(json, "enabled_mods", "0"),
            user_id: json_get_str(json, "user_id", "0"),
            time,
            replay_available: json_get_str(json, "replay_available", "0"),
            pp,
        })
    }

    pub fn as_leaderboard_entry(&self, pp_leaderboard: bool) -> LeaderboardEntry {
        let score_val = match (pp_leaderboard, self.pp) {
            (true, Some(pp)) => format!("{:.0}", pp),
            _ => self.score.clone(),
        };

        LeaderboardEntry {
            score_id: self.score_id.parse().unwrap_or(0),
            username: self.username.clone(),
            score: score_val.parse().unwrap_or(0),
            maxcombo: self.maxcombo.parse().unwrap_or(0),
            count50: self.count50.parse().unwrap_or(0),
            count100: self.count100.parse().unwrap_or(0),
            count300: self.count300.parse().unwrap_or(0),
            countmiss: self.countmiss.parse().unwrap_or(0),
            countkatu: self.countkatu.parse().unwrap_or(0),
            countgeki: self.countgeki.parse().unwrap_or(0),
            perfect: self.perfect.parse().unwrap_or(0),
            enabled_mods: self.enabled_mods.parse().unwrap_or(0),
            user_id: self.user_id.parse().unwrap_or(0),
            time: self.time,
            replay_available: self.replay_available.parse().unwrap_or(0),
        }
    }
}

fn chrono_parse_to_epoch(date_str: &str) -> i64 {
    let parts: Vec<&str> = date_str.split(['-', ' ', ':']).collect();
    if parts.len() >= 6 {
        let year: i64 = parts[0].parse().unwrap_or(2020);
        let month: i64 = parts[1].parse().unwrap_or(1);
        let day: i64 = parts[2].parse().unwrap_or(1);
        let hour: i64 = parts[3].parse().unwrap_or(0);
        let min: i64 = parts[4].parse().unwrap_or(0);
        let sec: i64 = parts[5].parse().unwrap_or(0);

        let month_days = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
        let m_idx = (month - 1).clamp(0, 11) as usize;

        let days = (year - 1970) * 365 + ((year - 1969) / 4) - ((year - 1901) / 100) + ((year - 1601) / 400) + month_days[m_idx] + day - 1;

        days * 86400 + hour * 3600 + min * 60 + sec
    } else {
        SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() as i64
    }
}

fn json_get_str(json: &serde_json::Value, key: &str, default: &str) -> String {
    match json.get(key) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Number(n)) => n.to_string(),
        _ => default.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bancho_score_from_json() {
        let json = serde_json::json!({
            "score_id": "12345678",
            "username": "WhiteCat",
            "score": "98765432",
            "maxcombo": "1500",
            "count50": "0",
            "count100": "5",
            "count300": "1000",
            "countmiss": "1",
            "countkatu": "2",
            "countgeki": "3",
            "perfect": "0",
            "enabled_mods": "24",
            "user_id": "450",
            "date": "2024-05-10 12:00:00",
            "replay_available": "1",
            "pp": "750.5"
        });

        let score = BanchoScore::from_json(&json).unwrap();
        assert_eq!(score.score_id, "12345678");
        assert_eq!(score.username, "WhiteCat");
        assert_eq!(score.pp, Some(750.5));
        assert_eq!(score.enabled_mods, "24");

        let entry_raw = score.as_leaderboard_entry(false);
        assert_eq!(entry_raw.score, 98765432);
        assert_eq!(entry_raw.username, "WhiteCat");

        let entry_pp = score.as_leaderboard_entry(true);
        assert_eq!(entry_pp.score, 750);
    }

    #[test]
    fn test_bancho_score_numeric_json() {
        let json = serde_json::json!({
            "score_id": 9999,
            "username": "Mrekk",
            "score": 1000000,
            "maxcombo": 500,
            "count50": 0,
            "count100": 0,
            "count300": 300,
            "countmiss": 0,
            "countkatu": 0,
            "countgeki": 0,
            "perfect": 1,
            "enabled_mods": 8,
            "user_id": 12345,
            "date": "2023-01-01 00:00:00",
            "replay_available": 1
        });

        let score = BanchoScore::from_json(&json).unwrap();
        assert_eq!(score.score_id, "9999");
        assert_eq!(score.username, "Mrekk");
        assert_eq!(score.score, "1000000");
    }
}
