use crate::packets;
use crate::types::mods::Mods;
use crate::types::score::Score;

pub struct Player {
    pub name: String,
    pub queue: Vec<u8>,

    pub rank: i32,
    pub acc: f64,
    pub playcount: i32,
    pub total_score: i64,
    pub ranked_score: i64,
    pub pp: i32,

    pub mode: u8,
    pub mods: u32,
    pub userid: i32,
    pub action: i8,
    pub map_id: i32,
    pub country: u8,
    pub map_md5: String,
    pub utc_offset: u8,
    pub info_text: String,
    pub location: (f32, f32),
    pub bancho_privs: i32,
}

impl Player {
    pub fn new(name: String) -> Self {
        Self {
            name,
            queue: Vec::new(),
            rank: 9999999,
            acc: 0.0,
            playcount: 0,
            total_score: 0,
            ranked_score: 0,
            pp: 0,
            mode: 0,
            mods: 0,
            userid: 2,
            action: 0,
            map_id: 0,
            country: 0,
            map_md5: String::new(),
            utc_offset: 0,
            info_text: String::new(),
            location: (0.0, 0.0),
            bancho_privs: 63,
        }
    }

    pub fn clear_queue(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.queue)
    }

    pub fn calculate_stats(&mut self, scores: &[Score], filter_mod: Option<Mods>, playcount: i32) {
        let current_mode = self.mode as i32;
        let mut filtered: Vec<&Score> = scores
            .iter()
            .filter(|s| s.mode == current_mode)
            .filter(|s| {
                if let Some(fm) = filter_mod {
                    Mods::from_bits_truncate(s.mods).intersects(fm)
                } else {
                    !Mods::from_bits_truncate(s.mods).intersects(Mods::RELAX | Mods::AUTOPILOT)
                }
            })
            .collect();

        filtered.sort_by(|a, b| b.pp.unwrap_or(0.0).partial_cmp(&a.pp.unwrap_or(0.0)).unwrap_or(std::cmp::Ordering::Equal));

        let mut seen_md5 = std::collections::HashSet::new();
        let top_scores: Vec<&&Score> = filtered.iter().filter(|s| seen_md5.insert(s.md5.clone())).take(100).collect();

        let mut pp: f64 = top_scores.iter().enumerate().map(|(i, s)| s.pp.unwrap_or(0.0) * 0.95_f64.powi(i as i32)).sum();

        // osu! bonus PP formula: scales with number of scores submitted up to ~416.67
        if !filtered.is_empty() {
            pp += 416.6667 * (1.0 - 0.9994_f64.powi(filtered.len() as i32));
        }
        self.pp = pp.round() as i32;

        if !top_scores.is_empty() {
            self.acc = top_scores.iter().map(|s| s.acc.unwrap_or(0.0)).sum::<f64>() / top_scores.len() as f64;
        } else {
            self.acc = 0.0;
        }

        let mut best_scores_per_map: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        let mut total = 0i64;
        for s in &filtered {
            total += s.score;
            let entry = best_scores_per_map.entry(s.md5.clone()).or_insert(0);
            if s.score > *entry {
                *entry = s.score;
            }
        }
        self.ranked_score = best_scores_per_map.values().sum();
        self.total_score = total;

        self.playcount = playcount;
    }

    pub fn enqueue_stats(&mut self) {
        let stats = packets::user_stats(self);
        self.queue.extend_from_slice(&stats);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_score(mode: i32, md5: &str, pp: f64, acc: f64, score_val: i64) -> Score {
        Score {
            mode,
            md5: md5.to_string(),
            name: "TestPlayer".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: score_val,
            max_combo: 500,
            perfect: true,
            mods: 0,
            time: 1000,
            acc: Some(acc),
            pp: Some(pp),
            replay_md5: None,
            scoreid: None,
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        }
    }

    #[test]
    fn test_mode_stats_isolation() {
        let mut player = Player::new("TestPlayer".to_string());

        let scores = vec![
            dummy_score(0, "std_map", 300.0, 99.0, 1_000_000),
            dummy_score(1, "taiko_map", 500.0, 95.0, 2_000_000),
            dummy_score(3, "mania_map", 700.0, 98.0, 1_000_000),
        ];

        // Mode 0: should only calculate standard
        player.mode = 0;
        player.calculate_stats(&scores, None, 10);
        assert!(player.pp >= 300);
        assert!((player.acc - 99.0).abs() < 1e-4);
        assert_eq!(player.ranked_score, 1_000_000);

        // Mode 1: switch to Taiko, should only calculate taiko
        player.mode = 1;
        player.calculate_stats(&scores, None, 10);
        assert!(player.pp >= 500);
        assert!((player.acc - 95.0).abs() < 1e-4);
        assert_eq!(player.ranked_score, 2_000_000);

        // Mode 2: switch to Catch (no plays yet), should have 0 stats
        player.mode = 2;
        player.calculate_stats(&scores, None, 10);
        assert_eq!(player.pp, 0);
        assert_eq!(player.acc, 0.0);
        assert_eq!(player.ranked_score, 0);

        // Mode 3: switch to Mania
        player.mode = 3;
        player.calculate_stats(&scores, None, 10);
        assert!(player.pp >= 700);
        assert!((player.acc - 98.0).abs() < 1e-4);
        assert_eq!(player.ranked_score, 1_000_000);
    }
}
