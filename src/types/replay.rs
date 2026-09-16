use crate::types::mods::Mods;

pub struct Replay {
    data: Vec<u8>,
    offset: usize,

    pub mode: u8,
    pub version: i32,
    pub beatmap_md5: String,
    pub player_name: String,
    pub replay_md5: String,
    pub n300: i16,
    pub n100: i16,
    pub n50: i16,
    pub ngeki: i16,
    pub nkatu: i16,
    pub nmiss: i16,
    pub total_score: i32,
    pub combo: i16,
    pub perfect: bool,
    pub mods: Mods,
    pub bar_graph_raw: String,
    pub timestamp: i64,
    pub raw_frames: Vec<u8>,
    pub score_id: i64,
}

pub struct LifeBar {
    pub delta_time: i32,
    pub current_hp: f64,
}

impl LifeBar {
    pub fn from_raw(raw: &str) -> Self {
        if !raw.contains(',') {
            return Self { delta_time: raw.parse().unwrap_or(0), current_hp: 1.0 };
        }
        let parts: Vec<&str> = raw.split(',').collect();
        let hp = parts[0].parse().unwrap_or(1.0);
        let time = parts.get(1).and_then(|t| t.parse().ok()).unwrap_or(0);
        Self { delta_time: time, current_hp: hp }
    }
}

impl Replay {
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, String> {
        let mut replay = Self {
            data,
            offset: 0,
            mode: 0,
            version: 0,
            beatmap_md5: String::new(),
            player_name: String::new(),
            replay_md5: String::new(),
            n300: 0,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            total_score: 0,
            combo: 0,
            perfect: false,
            mods: Mods::empty(),
            bar_graph_raw: String::new(),
            timestamp: 0,
            raw_frames: vec![],
            score_id: 0,
        };
        replay.parse()?;
        Ok(replay)
    }

    pub fn from_file(path: &std::path::Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("Failed to read replay: {e}"))?;
        Self::from_bytes(data)
    }

    pub fn parse_life_bars(&self) -> Vec<LifeBar> {
        self.bar_graph_raw.split('|').filter(|s| !s.is_empty()).map(LifeBar::from_raw).collect()
    }

    pub fn is_failed(&self) -> bool {
        if self.mods.intersects(Mods::RELAX | Mods::AUTOPILOT | Mods::NOFAIL) {
            return false;
        }
        for bar in self.parse_life_bars() {
            if bar.current_hp == 0.0 {
                return true;
            }
        }
        false
    }

    fn parse(&mut self) -> Result<(), String> {
        self.mode = self.read_byte()? as u8;
        self.version = self.read_i32()?;
        self.beatmap_md5 = self.read_osu_string()?;
        self.player_name = self.read_osu_string()?;
        self.replay_md5 = self.read_osu_string()?;
        self.n300 = self.read_i16()?;
        self.n100 = self.read_i16()?;
        self.n50 = self.read_i16()?;
        self.ngeki = self.read_i16()?;
        self.nkatu = self.read_i16()?;
        self.nmiss = self.read_i16()?;
        self.total_score = self.read_i32()?;
        self.combo = self.read_i16()?;
        self.perfect = self.read_byte()? != 0;
        self.mods = Mods::from_bits_truncate(self.read_i32()? as u32);
        self.bar_graph_raw = self.read_osu_string()?;
        self.timestamp = self.read_i64()?;

        let frame_len = self.read_i32()? as usize;
        self.raw_frames = self.read_raw(frame_len)?;

        self.score_id = self.read_i64()?;

        Ok(())
    }

    fn remaining(&self) -> &[u8] {
        &self.data[self.offset..]
    }

    fn read_byte(&mut self) -> Result<i8, String> {
        if self.remaining().is_empty() {
            return Err("Unexpected end of replay data".into());
        }
        let val = self.data[self.offset] as i8;
        self.offset += 1;
        Ok(val)
    }

    fn read_i16(&mut self) -> Result<i16, String> {
        if self.remaining().len() < 2 {
            return Err("Unexpected end of replay data".into());
        }
        let val = i16::from_le_bytes([self.data[self.offset], self.data[self.offset + 1]]);
        self.offset += 2;
        Ok(val)
    }

    fn read_i32(&mut self) -> Result<i32, String> {
        if self.remaining().len() < 4 {
            return Err("Unexpected end of replay data".into());
        }
        let val = i32::from_le_bytes([self.data[self.offset], self.data[self.offset + 1], self.data[self.offset + 2], self.data[self.offset + 3]]);
        self.offset += 4;
        Ok(val)
    }

    fn read_i64(&mut self) -> Result<i64, String> {
        if self.remaining().len() < 8 {
            return Err("Unexpected end of replay data".into());
        }
        let mut bytes = [0u8; 8];
        bytes.copy_from_slice(&self.data[self.offset..self.offset + 8]);
        let val = i64::from_le_bytes(bytes);
        self.offset += 8;
        Ok(val)
    }

    fn read_uleb128(&mut self) -> Result<u32, String> {
        let mut val: u32 = 0;
        let mut shift = 0;
        loop {
            if self.remaining().is_empty() {
                return Err("Unexpected end of replay data".into());
            }
            let b = self.data[self.offset];
            self.offset += 1;
            val |= ((b & 0x7F) as u32) << shift;
            if b & 0x80 == 0 {
                break;
            }
            shift += 7;
        }
        Ok(val)
    }

    fn read_osu_string(&mut self) -> Result<String, String> {
        let indicator = self.read_byte()?;
        if indicator == 0x0b {
            let len = self.read_uleb128()? as usize;
            let raw = self.read_raw(len)?;
            String::from_utf8(raw).map_err(|e| format!("Invalid UTF-8 in replay string: {e}"))
        } else {
            Ok(String::new())
        }
    }

    fn read_raw(&mut self, len: usize) -> Result<Vec<u8>, String> {
        if self.remaining().len() < len {
            return Err("Unexpected end of replay data".into());
        }
        let val = self.data[self.offset..self.offset + len].to_vec();
        self.offset += len;
        Ok(val)
    }
}
