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
    ChoChannelJoinSuccess = 64,
    ChoChannelInfo = 65,
    ChoPrivileges = 71,
    ChoFriendsList = 72,
    OsuFriendAdd = 73,
    OsuFriendRemove = 74,
    ChoProtocolVersion = 75,
    ChoMainMenuIcon = 76,
    OsuChannelJoin = 63,
    OsuChannelPart = 78,
    ChoUserPresence = 83,
    OsuUserStatsRequest = 85,
    ChoRestart = 86,
    ChoChannelInfoEnd = 89,
    ChoUserSilenced = 94,
    ChoUserPresenceBundle = 96,
    OsuUserPresenceRequest = 97,
    OsuUserPresenceRequestAll = 98,
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

fn write_i16(s: i16) -> Vec<u8> {
    s.to_le_bytes().to_vec()
}

fn write_i64(l: i64) -> Vec<u8> {
    l.to_le_bytes().to_vec()
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_osu_string() {
        let empty = write_string("");
        assert_eq!(empty, vec![0x00]);

        let str_bytes = write_string("osu!");
        assert_eq!(str_bytes[0], 0x0b);
        assert_eq!(str_bytes[1], 4);
        assert_eq!(&str_bytes[2..], b"osu!");
    }

    #[test]
    fn test_notification_packet() {
        let pkt = notification("Hello World");
        assert_eq!(pkt[0], 24);
        assert_eq!(pkt[1], 0);
        assert_eq!(pkt[2], 0);
        assert!(pkt.len() > 7);
    }

    #[test]
    fn test_user_id_packet() {
        let pkt = user_id(2);
        assert_eq!(pkt[0], 5);
        assert_eq!(pkt[1], 0);
        assert_eq!(pkt[2], 0);
        assert_eq!(&pkt[3..7], &4i32.to_le_bytes());
        assert_eq!(&pkt[7..11], &2i32.to_le_bytes());
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

    #[test]
    fn test_pong_packet() {
        let pkt = pong();
        assert_eq!(pkt[0], 8);
        assert_eq!(pkt[1], 0);
        assert_eq!(pkt[2], 0);
        assert_eq!(&pkt[3..7], &0u32.to_le_bytes());
    }
}
