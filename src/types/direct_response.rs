pub struct DirectResponse {
    pub entries: Vec<DirectEntry>,
}

pub struct DirectEntry {
    pub id: i64,
    pub artist: String,
    pub title: String,
    pub creator: String,
    pub ranked: i32,
    pub last_updated: String,
    pub diffs: Vec<String>,
}

impl DirectResponse {
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    pub fn as_binary(&self) -> Vec<u8> {
        let len = self.entries.len();
        let header = if len == 100 { "101".to_string() } else { len.to_string() };
        let mut lines = vec![header];

        for entry in &self.entries {
            let diffs_str = entry.diffs.join(",");
            // osu!direct pipe-delimited search response format:
            // filename|artist|title|creator|status|rating|last_updated|set_id|thread_id|has_video|has_storyboard|filesize|filesize_novideo|diffs
            let line = format!(
                "{}.osz|{}|{}|{}|{}|10.0|{}|{}|0|0|0|0|0|{}",
                entry.id, entry.artist, entry.title, entry.creator, entry.ranked, entry.last_updated, entry.id, diffs_str,
            );
            lines.push(line);
        }

        lines.join("\n").into_bytes()
    }

    pub fn from_str(s: &str, start_id: i64) -> Self {
        let mut resp = Self::new();
        let mut id = start_id;

        for line in s.lines() {
            resp.entries.push(DirectEntry {
                id,
                artist: line.to_string(),
                title: String::new(),
                creator: String::new(),
                ranked: 1,
                last_updated: "2020-01-01T00:00:00Z".to_string(),
                diffs: vec![String::new()],
            });
            id -= 1;
        }

        resp
    }

    pub fn from_plays(plays: &[PlayEntry]) -> Self {
        let mut resp = Self::new();

        for (idx, play) in plays.iter().enumerate() {
            let grade = crate::utils::get_grade(
                play.mode as u8,
                play.n300,
                play.n100,
                play.n50,
                play.ngeki,
                play.nkatu,
                play.nmiss,
                play.mods,
                play.acc,
            );

            let title = format!(
                "{} [{}] {:.0}PP +{} {:.2}% {} {}x/{}x {}X",
                play.bmap_title, play.bmap_version, play.pp, play.mods_str, play.acc, grade, play.max_combo, play.bmap_max_combo, play.nmiss,
            );

            let title = if title.len() > 100 { format!("{}...", &title[..90]) } else { title };

            let diff_version = format!(
                "{:.0}PP +{} {:.2}% {} {}x/{}x {}X@{}",
                play.pp, play.mods_str, play.acc, grade, play.max_combo, play.bmap_max_combo, play.nmiss, play.mode,
            );

            resp.entries.push(DirectEntry {
                id: -(play.bmap_setid),
                artist: format!("{}. {}", idx + 1, play.bmap_artist),
                title,
                creator: play.bmap_creator.clone(),
                ranked: play.bmap_approved,
                last_updated: play.time_str.clone(),
                diffs: vec![diff_version],
            });
        }

        resp
    }
}

pub struct PlayEntry {
    pub bmap_title: String,
    pub bmap_version: String,
    pub bmap_artist: String,
    pub bmap_creator: String,
    pub bmap_setid: i64,
    pub bmap_approved: i32,
    pub bmap_max_combo: i32,
    pub pp: f64,
    pub acc: f64,
    pub mods_str: String,
    pub max_combo: i32,
    pub nmiss: i32,
    pub n300: i32,
    pub n100: i32,
    pub n50: i32,
    pub ngeki: i32,
    pub nkatu: i32,
    pub mods: u32,
    pub mode: i32,
    pub time_str: String,
}
