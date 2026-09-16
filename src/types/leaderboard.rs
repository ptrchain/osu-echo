use crate::types::beatmap::Beatmap;
use crate::types::score::LeaderboardEntry;

pub const NOTSUBMITTED: i32 = -1;
pub const PENDING: i32 = 0;
pub const UPDATEAVAILABLE: i32 = 1;
pub const RANKED: i32 = 2;
pub const APPROVED: i32 = 3;
pub const QUALIFIED: i32 = 4;
pub const LOVED: i32 = 5;

pub const VALID_LB_STATUSES: [i32; 4] = [LOVED, QUALIFIED, RANKED, APPROVED];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeaderboardTypes {
    Local = 0,
    Top = 1,
    Mods = 2,
    Friends = 3,
    Country = 4,
}

impl LeaderboardTypes {
    pub fn from_i32(v: i32) -> Self {
        match v {
            0 => Self::Local,
            1 => Self::Top,
            2 => Self::Mods,
            3 => Self::Friends,
            4 => Self::Country,
            _ => Self::Top,
        }
    }
}

pub fn api_to_server_status(approved: i32) -> i32 {
    match approved {
        4 => LOVED,
        3 => QUALIFIED,
        2 => APPROVED,
        1 => RANKED,
        -2..=0 => PENDING,
        _ => PENDING,
    }
}

pub fn status_to_db_key(approved: i32) -> &'static str {
    match approved {
        1 => "ranked",
        2 => "approved",
        3 => "qualified",
        4 => "loved",
        _ => "ranked",
    }
}

pub struct Leaderboard {
    pub scores: Vec<LeaderboardEntry>,
    pub bmap: Option<Beatmap>,
    pub personal_score: Option<LeaderboardEntry>,
}

impl Leaderboard {
    pub fn new() -> Self {
        Self { scores: Vec::new(), bmap: None, personal_score: None }
    }

    pub fn as_binary(&self, amount_of_scores: i32, _show_pp_pb: bool) -> Vec<u8> {
        let Some(bmap) = &self.bmap else {
            return b"0|false".to_vec();
        };

        let r = api_to_server_status(bmap.approved);
        if !VALID_LB_STATUSES.contains(&r) {
            return format!("{}|false", r).into_bytes();
        }

        let artist = bmap.artist_unicode.as_deref().unwrap_or(&bmap.artist);
        let title = bmap.title_unicode.as_deref().unwrap_or(&bmap.title);

        // Bancho getscores format: [status]|false|[map_id]|[set_id]|[count]\n[offset]\n[title]\n[rating]\n[personal_score]\n[scores...]
        let base = format!("{}|false|{}|{}|{}\n0\n[bold:0,size:20]{}|{}\n10.0\n", r, bmap.beatmap_id, bmap.beatmapset_id, self.scores.len(), artist, title,);

        let mut buffer = base.into_bytes();

        if let Some(ps) = &self.personal_score {
            let num = if !self.scores.iter().any(|s| s.score_id == ps.score_id) {
                amount_of_scores + 1
            } else {
                self.scores.iter().position(|s| s.score_id == ps.score_id).map(|i| i as i32 + 1).unwrap_or(1)
            };
            buffer.extend_from_slice(ps.format(num).as_bytes());
            buffer.push(b'\n');
        } else {
            buffer.push(b'\n');
        }

        let len_scores = self.scores.len();
        for (idx, s) in self.scores.iter().enumerate() {
            buffer.extend_from_slice(s.format(idx as i32 + 1).as_bytes());
            if idx + 1 != len_scores {
                buffer.push(b'\n');
            }
        }

        buffer
    }
}
