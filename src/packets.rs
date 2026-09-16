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
    ChoProtocolVersion = 75,
    ChoMainMenuIcon = 76,
    ChoFriendsList = 72,
    ChoChannelInfoEnd = 89,
    ChoChannelJoinSuccess = 64,
    ChoChannelInfo = 65,
    ChoPrivileges = 71,
    ChoUserPresence = 83,
    ChoRestart = 86,
    ChoUserSilenced = 94,
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

fn write_i32(i: i32) -> Vec<u8> {
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
fn write_packet(packet_id: u16, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::with_capacity(7 + data.len());
    p.extend_from_slice(&packet_id.to_le_bytes());
    p.push(0);
    p.extend_from_slice(&(data.len() as u32).to_le_bytes());
    p.extend_from_slice(data);
    p
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

pub fn user_presence(p: &Player) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&write_i32(p.userid));
    data.extend_from_slice(&write_string(&p.name));
    data.extend_from_slice(&write_u8(p.utc_offset + 24));
    data.extend_from_slice(&write_u8(p.country));
    data.extend_from_slice(&write_u8((p.bancho_privs as u8) | (p.mode << 5)));
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
}
