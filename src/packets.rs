use crate::types::player::Player;

#[repr(u16)]
#[allow(dead_code)]
pub enum PacketId {
    OsuChangeAction = 0,
    OsuSendPublicMessage = 1,
    OsuLogout = 2,
    OsuRequestStatusUpdate = 3,
    OsuPing = 4,
    ChoUserId = 5,
    ChoSendMessage = 7,
    ChoPong = 8,
    ChoUserStats = 11,
    ChoUserLogout = 12,
    ChoVersionUpdate = 19,
    ChoNotification = 24,
    OsuSendPrivateMessage = 25,
    // Lobby
    ChoUpdateMatch = 26,
    ChoNewMatch = 27,
    ChoDisposeMatch = 28,
    OsuPartLobby = 29,
    OsuJoinLobby = 30,
    // Multiplayer room
    OsuCreateMatch = 31,
    OsuJoinMatch = 32,
    OsuPartMatch = 33,
    ChoMatchJoinSuccess = 36,
    ChoMatchJoinFail = 37,
    OsuMatchChangeSlot = 38,
    OsuMatchReady = 39,
    OsuMatchLock = 40,
    OsuMatchChangeSettings = 41,
    OsuMatchStart = 44,
    ChoMatchStart = 46,
    // Multiplayer gameplay
    OsuMatchScoreUpdate = 47,
    ChoMatchScoreUpdate = 48,
    OsuMatchComplete = 49,
    ChoMatchTransferHost = 50,
    OsuMatchChangeMods = 51,
    OsuMatchLoadComplete = 52,
    ChoMatchAllPlayersLoaded = 53,
    OsuMatchNoBeatmap = 54,
    OsuMatchNotReady = 55,
    OsuMatchFailed = 56,
    ChoMatchPlayerFailed = 57,
    ChoMatchComplete = 58,
    OsuMatchHasBeatmap = 59,
    OsuMatchSkipRequest = 60,
    ChoMatchSkip = 61,
    OsuChannelJoin = 63,
    ChoChannelJoinSuccess = 64,
    ChoChannelInfo = 65,
    ChoChannelKick = 66,
    OsuMatchTransferHost = 70,
    ChoPrivileges = 71,
    ChoFriendsList = 72,
    OsuFriendAdd = 73,
    OsuFriendRemove = 74,
    ChoProtocolVersion = 75,
    ChoMainMenuIcon = 76,
    OsuMatchChangeTeam = 77,
    OsuChannelPart = 78,
    ChoUserPresence = 83,
    OsuUserStatsRequest = 85,
    ChoRestart = 86,
    ChoChannelInfoEnd = 89,
    OsuMatchChangePassword = 90,
    ChoUserSilenced = 94,
    ChoUserPresenceBundle = 96,
    OsuUserPresenceRequest = 97,
    OsuUserPresenceRequestAll = 98,
    ChoMatchAbort = 106,
}

/// Bitmask of Bancho slot statuses that carry a player user_id in MatchWire:
/// NotReady (4) | Ready (8) | NoMap (16) | Playing (32) | Complete (64) = 124.
#[allow(dead_code)]
pub const MATCH_SLOT_HAS_PLAYER: u8 = 124;

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MatchWire {
    pub match_id: u16,
    pub in_progress: u8,
    pub match_type: u8,
    pub mods: u32,
    pub name: String,
    pub password: String,
    pub beatmap_name: String,
    pub beatmap_id: i32,
    pub beatmap_md5: String,
    pub slot_status: [u8; 16],
    pub slot_team: [u8; 16],
    pub user_ids: Vec<i32>,
    pub host_id: i32,
    pub mode: u8,
    pub win_condition: u8,
    pub team_type: u8,
    pub freemods: u8,
    pub slot_mods: [u32; 16],
    pub seed: i32,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScoreFrame {
    pub time: i32,
    pub id: u8,
    pub n300: u16,
    pub n100: u16,
    pub n50: u16,
    pub ngeki: u16,
    pub nkatu: u16,
    pub nmiss: u16,
    pub total_score: i32,
    pub max_combo: u16,
    pub current_combo: u16,
    pub perfect: bool,
    pub current_hp: u8,
    pub tag_byte: u8,
    pub using_score_v2: bool,
    pub combo_portion: f64,
    pub bonus_portion: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InPacket<'a> {
    pub id: u16,
    pub payload: &'a [u8],
}

pub struct PacketReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> PacketReader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    pub fn is_empty(&self) -> bool {
        self.pos >= self.data.len()
    }

    pub fn take(&mut self, len: usize) -> Result<&'a [u8], &'static str> {
        if self.pos + len > self.data.len() {
            return Err("Unexpected end of packet buffer");
        }
        let slice = &self.data[self.pos..self.pos + len];
        self.pos += len;
        Ok(slice)
    }

    pub fn read_u8(&mut self) -> Result<u8, &'static str> {
        let b = self.take(1)?;
        Ok(b[0])
    }

    pub fn read_i8(&mut self) -> Result<i8, &'static str> {
        Ok(self.read_u8()? as i8)
    }

    pub fn read_u16(&mut self) -> Result<u16, &'static str> {
        let b = self.take(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn read_i16(&mut self) -> Result<i16, &'static str> {
        let b = self.take(2)?;
        Ok(i16::from_le_bytes([b[0], b[1]]))
    }

    pub fn read_u32(&mut self) -> Result<u32, &'static str> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_i32(&mut self) -> Result<i32, &'static str> {
        let b = self.take(4)?;
        Ok(i32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn read_f32(&mut self) -> Result<f32, &'static str> {
        let b = self.take(4)?;
        Ok(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    #[allow(dead_code)]
    pub fn read_u64(&mut self) -> Result<u64, &'static str> {
        let b = self.take(8)?;
        Ok(u64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    #[allow(dead_code)]
    pub fn read_f64(&mut self) -> Result<f64, &'static str> {
        let b = self.take(8)?;
        Ok(f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]))
    }

    pub fn read_uleb128(&mut self) -> Result<usize, &'static str> {
        let mut result = 0usize;
        let mut shift = 0;
        loop {
            let byte = self.read_u8()?;
            result |= ((byte & 0x7F) as usize) << shift;
            if (byte & 0x80) == 0 {
                break;
            }
            shift += 7;
            if shift >= 35 {
                return Err("ULEB128 overflow");
            }
        }
        Ok(result)
    }

    pub fn read_string(&mut self) -> Result<String, &'static str> {
        let marker = self.read_u8()?;
        if marker == 0x00 {
            return Ok(String::new());
        }
        if marker != 0x0b {
            return Err("Invalid string marker, expected 0x00 or 0x0b");
        }
        let len = self.read_uleb128()?;
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).map_err(|_| "Invalid UTF-8 in string")
    }

    pub fn read_i32_list(&mut self) -> Result<Vec<i32>, &'static str> {
        let count = self.read_i16()?;
        if count < 0 {
            return Err("Negative list count");
        }
        let mut list = Vec::with_capacity(count as usize);
        for _ in 0..count {
            list.push(self.read_i32()?);
        }
        Ok(list)
    }

    #[allow(dead_code)]
    pub fn read_match(&mut self) -> Result<MatchWire, &'static str> {
        let match_id = self.read_u16()?;
        let in_progress = self.read_u8()?;
        let match_type = self.read_u8()?;
        let mods = self.read_u32()?;
        let name = self.read_string()?;
        let password = self.read_string()?;
        let beatmap_name = self.read_string()?;
        let beatmap_id = self.read_i32()?;
        let beatmap_md5 = self.read_string()?;

        let mut slot_status = [0u8; 16];
        slot_status.copy_from_slice(self.take(16)?);
        let mut slot_team = [0u8; 16];
        slot_team.copy_from_slice(self.take(16)?);

        let mut user_ids = Vec::new();
        for &status in &slot_status {
            if (status & MATCH_SLOT_HAS_PLAYER) != 0 {
                user_ids.push(self.read_i32()?);
            }
        }

        let host_id = self.read_i32()?;
        let mode = self.read_u8()?;
        let win_condition = self.read_u8()?;
        let team_type = self.read_u8()?;
        let freemods = self.read_u8()?;

        let mut slot_mods = [0u32; 16];
        if freemods != 0 {
            for slot_mod in &mut slot_mods {
                *slot_mod = self.read_u32()?;
            }
        }

        let seed = self.read_i32()?;

        Ok(MatchWire {
            match_id,
            in_progress,
            match_type,
            mods,
            name,
            password,
            beatmap_name,
            beatmap_id,
            beatmap_md5,
            slot_status,
            slot_team,
            user_ids,
            host_id,
            mode,
            win_condition,
            team_type,
            freemods,
            slot_mods,
            seed,
        })
    }

    #[allow(dead_code)]
    pub fn read_score_frame(&mut self) -> Result<ScoreFrame, &'static str> {
        let time = self.read_i32()?;
        let id = self.read_u8()?;
        let n300 = self.read_u16()?;
        let n100 = self.read_u16()?;
        let n50 = self.read_u16()?;
        let ngeki = self.read_u16()?;
        let nkatu = self.read_u16()?;
        let nmiss = self.read_u16()?;
        let total_score = self.read_i32()?;
        let max_combo = self.read_u16()?;
        let current_combo = self.read_u16()?;
        let perfect = self.read_u8()? != 0;
        let current_hp = self.read_u8()?;
        let tag_byte = self.read_u8()?;
        let using_score_v2 = self.read_u8()? != 0;
        let (combo_portion, bonus_portion) = if using_score_v2 {
            (self.read_f64()?, self.read_f64()?)
        } else {
            (0.0, 0.0)
        };

        Ok(ScoreFrame {
            time,
            id,
            n300,
            n100,
            n50,
            ngeki,
            nkatu,
            nmiss,
            total_score,
            max_combo,
            current_combo,
            perfect,
            current_hp,
            tag_byte,
            using_score_v2,
            combo_portion,
            bonus_portion,
        })
    }
}

pub fn split_packets(mut data: &[u8]) -> Vec<InPacket<'_>> {
    let mut packets = Vec::new();
    while data.len() >= 7 {
        let id = u16::from_le_bytes([data[0], data[1]]);
        let len = u32::from_le_bytes([data[3], data[4], data[5], data[6]]) as usize;
        if data.len() < 7 + len {
            break;
        }
        packets.push(InPacket { id, payload: &data[7..7 + len] });
        data = &data[7 + len..];
    }
    packets
}

fn write_uleb128(num: u32) -> Vec<u8> {
    if num == 0 {
        return vec![0];
    }
    let mut result = Vec::new();
    let mut n = num;
    while n > 0 {
        let mut byte = (n & 0x7F) as u8;
        n >>= 7;
        if n != 0 {
            byte |= 0x80;
        }
        result.push(byte);
    }
    result
}

fn write_string(s: &str) -> Vec<u8> {
    if s.is_empty() {
        return vec![0x00];
    }
    let bytes = s.as_bytes();
    let mut result = vec![0x0b];
    result.extend_from_slice(&write_uleb128(bytes.len() as u32));
    result.extend_from_slice(bytes);
    result
}

pub fn write_i32(i: i32) -> Vec<u8> {
    i.to_le_bytes().to_vec()
}

fn write_u32(i: u32) -> Vec<u8> {
    i.to_le_bytes().to_vec()
}

fn write_f32(f: f32) -> Vec<u8> {
    f.to_le_bytes().to_vec()
}

fn write_i8(b: i8) -> Vec<u8> {
    b.to_le_bytes().to_vec()
}

fn write_u8(b: u8) -> Vec<u8> {
    vec![b]
}

fn write_u16(s: u16) -> Vec<u8> {
    s.to_le_bytes().to_vec()
}

fn write_i16(s: i16) -> Vec<u8> {
    s.to_le_bytes().to_vec()
}

fn write_i64(l: i64) -> Vec<u8> {
    l.to_le_bytes().to_vec()
}

fn write_f64(f: f64) -> Vec<u8> {
    f.to_le_bytes().to_vec()
}

#[allow(dead_code)]
pub fn write_match_bytes(m: &MatchWire, send_password: bool) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_u16(m.match_id));
    data.push(m.in_progress);
    data.push(m.match_type);
    data.extend_from_slice(&write_u32(m.mods));
    data.extend_from_slice(&write_string(&m.name));
    let pw = if send_password {
        m.password.as_str()
    } else if !m.password.is_empty() {
        " "
    } else {
        ""
    };
    data.extend_from_slice(&write_string(pw));
    data.extend_from_slice(&write_string(&m.beatmap_name));
    data.extend_from_slice(&write_i32(m.beatmap_id));
    data.extend_from_slice(&write_string(&m.beatmap_md5));
    data.extend_from_slice(&m.slot_status);
    data.extend_from_slice(&m.slot_team);

    let mut uid_iter = m.user_ids.iter();
    for &status in &m.slot_status {
        if (status & MATCH_SLOT_HAS_PLAYER) != 0 {
            let uid = uid_iter.next().copied().unwrap_or(0);
            data.extend_from_slice(&write_i32(uid));
        }
    }

    data.extend_from_slice(&write_i32(m.host_id));
    data.push(m.mode);
    data.push(m.win_condition);
    data.push(m.team_type);
    data.push(m.freemods);

    if m.freemods != 0 {
        for &slot_mod in &m.slot_mods {
            data.extend_from_slice(&write_u32(slot_mod));
        }
    }

    data.extend_from_slice(&write_i32(m.seed));
    data
}

#[allow(dead_code)]
pub fn write_score_frame_bytes(frame: &ScoreFrame) -> Vec<u8> {
    let mut data = Vec::with_capacity(if frame.using_score_v2 { 45 } else { 29 });
    data.extend_from_slice(&write_i32(frame.time));
    data.push(frame.id);
    data.extend_from_slice(&write_u16(frame.n300));
    data.extend_from_slice(&write_u16(frame.n100));
    data.extend_from_slice(&write_u16(frame.n50));
    data.extend_from_slice(&write_u16(frame.ngeki));
    data.extend_from_slice(&write_u16(frame.nkatu));
    data.extend_from_slice(&write_u16(frame.nmiss));
    data.extend_from_slice(&write_i32(frame.total_score));
    data.extend_from_slice(&write_u16(frame.max_combo));
    data.extend_from_slice(&write_u16(frame.current_combo));
    data.push(u8::from(frame.perfect));
    data.push(frame.current_hp);
    data.push(frame.tag_byte);
    data.push(u8::from(frame.using_score_v2));
    if frame.using_score_v2 {
        data.extend_from_slice(&write_f64(frame.combo_portion));
        data.extend_from_slice(&write_f64(frame.bonus_portion));
    }
    data
}

fn write_list32(list: &[i32]) -> Vec<u8> {
    let mut result = write_i16(list.len() as i16);
    for item in list {
        result.extend_from_slice(&write_i32(*item));
    }
    result
}

// Bancho packet wire layout: [packet_id: u16 le][padding: 1 byte (0x00)][length: u32 le][payload: bytes]
pub fn write_packet(packet_id: u16, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(7 + data.len());
    p.extend_from_slice(&packet_id.to_le_bytes());
    p.push(0);
    p.extend_from_slice(&(data.len() as u32).to_le_bytes());
    p.extend_from_slice(data);
    p
}

pub fn pong() -> Vec<u8> {
    write_packet(PacketId::ChoPong as u16, &[])
}

pub fn user_id(id: i32) -> Vec<u8> {
    let data = if id > 0 { write_u32(id as u32) } else { write_i32(id) };
    write_packet(PacketId::ChoUserId as u16, &data)
}

pub fn notification(msg: &str) -> Vec<u8> {
    write_packet(PacketId::ChoNotification as u16, &write_string(msg))
}

pub fn protocol_version(version: i32) -> Vec<u8> {
    write_packet(PacketId::ChoProtocolVersion as u16, &write_i32(version))
}

pub fn bancho_privs(privs: i32) -> Vec<u8> {
    write_packet(PacketId::ChoPrivileges as u16, &write_i32(privs))
}

pub fn user_presence_bundle(user_ids: &[i32]) -> Vec<u8> {
    write_packet(PacketId::ChoUserPresenceBundle as u16, &write_list32(user_ids))
}

pub fn user_presence(p: &Player) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_i32(p.userid));
    data.extend_from_slice(&write_string(&p.name));
    data.extend_from_slice(&write_u8(p.utc_offset + 24));
    data.extend_from_slice(&write_u8(p.country));
    data.extend_from_slice(&write_u8((p.bancho_privs as u8 & 0x1f) | (p.mode << 5)));
    data.extend_from_slice(&write_f32(p.location.0));
    data.extend_from_slice(&write_f32(p.location.1));
    data.extend_from_slice(&write_i32(p.rank));
    write_packet(PacketId::ChoUserPresence as u16, &data)
}

pub fn user_stats(p: &Player) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_i32(p.userid));
    data.extend_from_slice(&write_i8(p.action));
    data.extend_from_slice(&write_string(&p.info_text));
    data.extend_from_slice(&write_string(&p.map_md5));
    data.extend_from_slice(&write_i32(p.mods as i32));
    data.extend_from_slice(&write_u8(p.mode));
    data.extend_from_slice(&write_i32(p.map_id));
    data.extend_from_slice(&write_i64(p.ranked_score));
    data.extend_from_slice(&write_f32((p.acc / 100.0) as f32));
    data.extend_from_slice(&write_i32(p.playcount));
    data.extend_from_slice(&write_i64(p.total_score));
    data.extend_from_slice(&write_i32(p.rank));
    data.extend_from_slice(&write_i16(p.pp as i16));
    write_packet(PacketId::ChoUserStats as u16, &data)
}

pub fn menu_icon(image_link: &str, click_link: &str) -> Vec<u8> {
    let combined = format!("{}|{}", image_link, click_link);
    write_packet(PacketId::ChoMainMenuIcon as u16, &write_string(&combined))
}

pub fn friends_list(friends: &[i32]) -> Vec<u8> {
    write_packet(PacketId::ChoFriendsList as u16, &write_list32(friends))
}

pub fn channel_info_end() -> Vec<u8> {
    write_packet(PacketId::ChoChannelInfoEnd as u16, &[])
}

pub fn channel_join(name: &str) -> Vec<u8> {
    write_packet(PacketId::ChoChannelJoinSuccess as u16, &write_string(name))
}

pub fn channel_info(name: &str, desc: &str, count: i16) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_string(name));
    data.extend_from_slice(&write_string(desc));
    data.extend_from_slice(&write_i16(count));
    write_packet(PacketId::ChoChannelInfo as u16, &data)
}

pub fn system_restart(ms: i32) -> Vec<u8> {
    write_packet(PacketId::ChoRestart as u16, &write_i32(ms))
}

pub fn logout(uid: i32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_i32(uid));
    data.extend_from_slice(&write_u8(0));
    write_packet(PacketId::ChoUserLogout as u16, &data)
}

pub fn send_msg(client: &str, msg: &str, target: &str, userid: i32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_string(client));
    data.extend_from_slice(&write_string(msg));
    data.extend_from_slice(&write_string(target));
    data.extend_from_slice(&write_i32(userid));
    write_packet(PacketId::ChoSendMessage as u16, &data)
}

pub fn local_message(message: &str, channel: &str) -> Vec<u8> {
    send_msg("local", message, channel, -1)
}

#[allow(dead_code)]
pub fn channel_kick(channel: &str) -> Vec<u8> {
    write_packet(PacketId::ChoChannelKick as u16, &write_string(channel))
}

#[allow(dead_code)]
pub fn new_match(m: &MatchWire, send_pw: bool) -> Vec<u8> {
    write_packet(PacketId::ChoNewMatch as u16, &write_match_bytes(m, send_pw))
}

#[allow(dead_code)]
pub fn update_match(m: &MatchWire, send_pw: bool) -> Vec<u8> {
    write_packet(PacketId::ChoUpdateMatch as u16, &write_match_bytes(m, send_pw))
}

#[allow(dead_code)]
pub fn dispose_match(id: i32) -> Vec<u8> {
    write_packet(PacketId::ChoDisposeMatch as u16, &write_i32(id))
}

#[allow(dead_code)]
pub fn match_join_success(m: &MatchWire) -> Vec<u8> {
    write_packet(PacketId::ChoMatchJoinSuccess as u16, &write_match_bytes(m, true))
}

#[allow(dead_code)]
pub fn match_join_fail() -> Vec<u8> {
    write_packet(PacketId::ChoMatchJoinFail as u16, &[])
}

#[allow(dead_code)]
pub fn match_start(m: &MatchWire) -> Vec<u8> {
    write_packet(PacketId::ChoMatchStart as u16, &write_match_bytes(m, true))
}

#[allow(dead_code)]
pub fn match_all_players_loaded() -> Vec<u8> {
    write_packet(PacketId::ChoMatchAllPlayersLoaded as u16, &[])
}

#[allow(dead_code)]
pub fn match_score_update(frame: &ScoreFrame) -> Vec<u8> {
    write_packet(PacketId::ChoMatchScoreUpdate as u16, &write_score_frame_bytes(frame))
}

#[allow(dead_code)]
pub fn match_player_failed(slot_id: i32) -> Vec<u8> {
    write_packet(PacketId::ChoMatchPlayerFailed as u16, &write_i32(slot_id))
}

#[allow(dead_code)]
pub fn match_complete() -> Vec<u8> {
    write_packet(PacketId::ChoMatchComplete as u16, &[])
}

#[allow(dead_code)]
pub fn match_skip() -> Vec<u8> {
    write_packet(PacketId::ChoMatchSkip as u16, &[])
}

#[allow(dead_code)]
pub fn match_abort() -> Vec<u8> {
    write_packet(PacketId::ChoMatchAbort as u16, &[])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_builders() {
        assert_eq!(write_string(""), vec![0x00]);
        let str_bytes = write_string("osu!");
        assert_eq!(str_bytes[0], 0x0b);
        assert_eq!(str_bytes[1], 4);
        assert_eq!(&str_bytes[2..], b"osu!");

        let notif_pkt = notification("Hello World");
        assert_eq!(notif_pkt[0], 24);
        assert_eq!(notif_pkt[1], 0);
        assert_eq!(notif_pkt[2], 0);
        assert!(notif_pkt.len() > 7);

        let uid_pkt = user_id(2);
        assert_eq!(uid_pkt[0], 5);
        assert_eq!(&uid_pkt[3..7], &4i32.to_le_bytes());
        assert_eq!(&uid_pkt[7..11], &2i32.to_le_bytes());

        let pong_pkt = pong();
        assert_eq!(pong_pkt[0], 8);
        assert_eq!(&pong_pkt[3..7], &0u32.to_le_bytes());
    }

    #[test]
    fn test_packet_reader_primitives_and_strings() {
        let mut buf = Vec::new();
        buf.push(42u8);
        buf.push(-5i8 as u8);
        buf.extend_from_slice(&1234u16.to_le_bytes());
        buf.extend_from_slice(&(-999i16).to_le_bytes());
        buf.extend_from_slice(&50000u32.to_le_bytes());
        buf.extend_from_slice(&(-123456i32).to_le_bytes());
        buf.extend_from_slice(&1.25f32.to_le_bytes());
        buf.extend_from_slice(&9_876_543_210u64.to_le_bytes());
        buf.extend_from_slice(&3.141592653589793f64.to_le_bytes());
        buf.extend_from_slice(&write_string("test string"));
        buf.extend_from_slice(&write_string(""));
        buf.extend_from_slice(&write_list32(&[10, 20, 30]));

        let mut reader = PacketReader::new(&buf);
        assert_eq!(reader.read_u8().unwrap(), 42);
        assert_eq!(reader.read_i8().unwrap(), -5);
        assert_eq!(reader.read_u16().unwrap(), 1234);
        assert_eq!(reader.read_i16().unwrap(), -999);
        assert_eq!(reader.read_u32().unwrap(), 50000);
        assert_eq!(reader.read_i32().unwrap(), -123456);
        assert_eq!(reader.read_f32().unwrap(), 1.25);
        assert_eq!(reader.read_u64().unwrap(), 9_876_543_210);
        assert_eq!(reader.read_f64().unwrap(), 3.141592653589793);
        assert_eq!(reader.read_string().unwrap(), "test string");
        assert_eq!(reader.read_string().unwrap(), "");
        assert_eq!(reader.read_i32_list().unwrap(), vec![10, 20, 30]);
        assert!(reader.is_empty());
    }

    #[test]
    fn test_split_packets_roundtrip() {
        let p1 = write_packet(PacketId::OsuPing as u16, &[]);
        let p2 = write_packet(PacketId::OsuSendPublicMessage as u16, b"payload123");
        let mut combined = Vec::new();
        combined.extend_from_slice(&p1);
        combined.extend_from_slice(&p2);

        let split = split_packets(&combined);
        assert_eq!(split.len(), 2);
        assert_eq!(split[0].id, PacketId::OsuPing as u16);
        assert_eq!(split[0].payload, &[] as &[u8]);
        assert_eq!(split[1].id, PacketId::OsuSendPublicMessage as u16);
        assert_eq!(split[1].payload, b"payload123");
    }

    fn sample_match(freemods: u8, password: &str) -> MatchWire {
        let mut slot_status = [1u8; 16]; // Open
        slot_status[0] = 8; // Ready
        slot_status[1] = 32; // Playing
        slot_status[2] = 4; // NotReady
        slot_status[3] = 2; // Locked
        slot_status[4] = 16; // NoMap
        slot_status[5] = 64; // Complete
        slot_status[6] = 128; // Quit (not HAS_PLAYER)

        let mut slot_team = [0u8; 16];
        slot_team[0] = 1; // Blue
        slot_team[1] = 2; // Red
        slot_team[2] = 1; // Blue

        let mut slot_mods = [0u32; 16];
        if freemods != 0 {
            slot_mods[0] = 8; // HD
            slot_mods[1] = 16; // HR
            slot_mods[2] = 24; // HDHR
        }

        MatchWire {
            match_id: 42,
            in_progress: 1,
            match_type: 0,
            mods: 64, // DT
            name: "Simulated MP Room".to_string(),
            password: password.to_string(),
            beatmap_name: "Artist - Title [Diff]".to_string(),
            beatmap_id: 123456,
            beatmap_md5: "0123456789abcdef0123456789abcdef".to_string(),
            slot_status,
            slot_team,
            // Slots 0, 1, 2, 4, 5 have HAS_PLAYER (5 occupied slots; slots 3 & 6 do not)
            user_ids: vec![2, 1_000_000_001, 7562902, 1_000_000_002, 1_000_000_003],
            host_id: 2,
            mode: 0,
            win_condition: 3, // ScoreV2
            team_type: 2,     // TeamVs
            freemods,
            slot_mods,
            seed: 998877,
        }
    }

    #[test]
    fn test_match_wire_roundtrip_no_freemods_and_password_masking() {
        let m = sample_match(0, "secret123");

        // send_password = true preserves literal password
        let bytes_literal = write_match_bytes(&m, true);
        let mut reader = PacketReader::new(&bytes_literal);
        let decoded = reader.read_match().unwrap();
        assert!(reader.is_empty());
        assert_eq!(decoded, m);

        // send_password = false masks non-empty password as " "
        let bytes_masked = write_match_bytes(&m, false);
        let mut reader = PacketReader::new(&bytes_masked);
        let decoded_masked = reader.read_match().unwrap();
        assert!(reader.is_empty());
        assert_eq!(decoded_masked.password, " ");
        assert_eq!(decoded_masked.user_ids, m.user_ids);
        assert_eq!(decoded_masked.slot_mods, [0u32; 16]);

        // send_password = false with empty password stays ""
        let m_empty_pw = sample_match(0, "");
        let bytes_empty = write_match_bytes(&m_empty_pw, false);
        let mut reader = PacketReader::new(&bytes_empty);
        let decoded_empty = reader.read_match().unwrap();
        assert!(reader.is_empty());
        assert_eq!(decoded_empty.password, "");
    }

    #[test]
    fn test_match_wire_roundtrip_with_freemods() {
        let m = sample_match(1, "pw");
        let pkt = match_join_success(&m);
        let packets = split_packets(&pkt);
        assert_eq!(packets.len(), 1);
        assert_eq!(packets[0].id, PacketId::ChoMatchJoinSuccess as u16);

        let mut reader = PacketReader::new(packets[0].payload);
        let decoded = reader.read_match().unwrap();
        assert!(reader.is_empty());
        assert_eq!(decoded, m);
        assert_eq!(decoded.slot_mods[0], 8);
        assert_eq!(decoded.slot_mods[1], 16);
        assert_eq!(decoded.slot_mods[2], 24);
    }

    #[test]
    fn test_score_frame_roundtrip_v1_and_v2() {
        let v1 = ScoreFrame {
            time: 45_200,
            id: 3,
            n300: 210,
            n100: 12,
            n50: 1,
            ngeki: 45,
            nkatu: 4,
            nmiss: 2,
            total_score: 1_234_567,
            max_combo: 180,
            current_combo: 42,
            perfect: false,
            current_hp: 175,
            tag_byte: 0,
            using_score_v2: false,
            combo_portion: 0.0,
            bonus_portion: 0.0,
        };

        let pkt_v1 = match_score_update(&v1);
        let split_v1 = split_packets(&pkt_v1);
        assert_eq!(split_v1.len(), 1);
        assert_eq!(split_v1[0].id, PacketId::ChoMatchScoreUpdate as u16);
        assert_eq!(split_v1[0].payload.len(), 29);

        let mut r1 = PacketReader::new(split_v1[0].payload);
        assert_eq!(r1.read_score_frame().unwrap(), v1);
        assert!(r1.is_empty());

        let v2 = ScoreFrame {
            using_score_v2: true,
            perfect: true,
            combo_portion: 0.654321,
            bonus_portion: 0.298765,
            ..v1
        };

        let pkt_v2 = match_score_update(&v2);
        let split_v2 = split_packets(&pkt_v2);
        assert_eq!(split_v2.len(), 1);
        assert_eq!(split_v2[0].payload.len(), 45);

        let mut r2 = PacketReader::new(split_v2[0].payload);
        assert_eq!(r2.read_score_frame().unwrap(), v2);
        assert!(r2.is_empty());
    }

    #[test]
    fn test_multiplayer_packet_builders() {
        let m = sample_match(0, "pw");
        let mut combined = Vec::new();
        combined.extend_from_slice(&new_match(&m, false));
        combined.extend_from_slice(&update_match(&m, false));
        combined.extend_from_slice(&dispose_match(42));
        combined.extend_from_slice(&match_join_fail());
        combined.extend_from_slice(&match_start(&m));
        combined.extend_from_slice(&match_all_players_loaded());
        combined.extend_from_slice(&match_player_failed(5));
        combined.extend_from_slice(&match_complete());
        combined.extend_from_slice(&match_skip());
        combined.extend_from_slice(&match_abort());
        combined.extend_from_slice(&channel_kick("#multiplayer"));

        let pkts = split_packets(&combined);
        assert_eq!(pkts.len(), 11);
        assert_eq!(pkts[0].id, PacketId::ChoNewMatch as u16);
        assert_eq!(pkts[1].id, PacketId::ChoUpdateMatch as u16);
        assert_eq!(pkts[2].id, PacketId::ChoDisposeMatch as u16);
        assert_eq!(PacketReader::new(pkts[2].payload).read_i32().unwrap(), 42);
        assert_eq!(pkts[3].id, PacketId::ChoMatchJoinFail as u16);
        assert!(pkts[3].payload.is_empty());
        assert_eq!(pkts[4].id, PacketId::ChoMatchStart as u16);
        assert_eq!(PacketReader::new(pkts[4].payload).read_match().unwrap().password, "pw");
        assert_eq!(pkts[5].id, PacketId::ChoMatchAllPlayersLoaded as u16);
        assert_eq!(pkts[6].id, PacketId::ChoMatchPlayerFailed as u16);
        assert_eq!(PacketReader::new(pkts[6].payload).read_i32().unwrap(), 5);
        assert_eq!(pkts[7].id, PacketId::ChoMatchComplete as u16);
        assert_eq!(pkts[8].id, PacketId::ChoMatchSkip as u16);
        assert_eq!(pkts[9].id, PacketId::ChoMatchAbort as u16);
        assert_eq!(pkts[10].id, PacketId::ChoChannelKick as u16);
        assert_eq!(PacketReader::new(pkts[10].payload).read_string().unwrap(), "#multiplayer");
    }
}
