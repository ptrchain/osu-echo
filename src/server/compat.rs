//! Compatibility layer for older osu!stable clients (2022 and earlier).
//!
//! The 2022 builds speak the same bancho protocol as current ones, but in practice they
//! differ in three ways that matter to a local server:
//!
//! 1. Transport/routing: a 2022 client is normally pointed at a *single plain-HTTP host*
//!    (`osu!.exe -devserver http://127.0.0.1:5000`) instead of the `https://*.localhost`
//!    subdomain layout, so every request has to be routable on one port without TLS and
//!    without a trusted certificate. That is handled by `router` + `tls`, this module only
//!    records which client is connected.
//! 2. Endpoints: older builds call a handful of osu-web endpoints that current builds no
//!    longer use (and vice versa). `canonical_web_path` maps those aliases onto the handlers
//!    that actually exist so an old build never gets a hard 404.
//! 3. Packet leniency: the inbound stream must tolerate the field widths different client
//!    generations emit (for example the sender id in chat packets, which is a 4-byte int in
//!    older builds and an 8-byte long in newer ones). `parse_chat_payload` handles both.

/// Builds before this date are treated as "legacy" (2022 and earlier).
pub const LEGACY_BUILD_CUTOFF: u32 = 20230101;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientGeneration {
    /// Build could not be determined (no version line in the login body).
    Unknown,
    /// 2022 or earlier.
    Legacy,
    /// 2023 or newer.
    Modern,
}

#[derive(Debug, Clone, Copy)]
pub struct ClientInfo {
    pub build: Option<u32>,
    pub generation: ClientGeneration,
}

impl Default for ClientInfo {
    fn default() -> Self {
        Self::unknown()
    }
}

impl ClientInfo {
    pub fn unknown() -> Self {
        Self { build: None, generation: ClientGeneration::Unknown }
    }

    /// Parses a client version line such as `20220319`, `20220319.2` or `20230520.0`.
    pub fn from_version_line(line: &str) -> Self {
        let digits: String = line.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
        if digits.len() >= 8 {
            if let Ok(build) = digits[..8].parse::<u32>() {
                let generation = if build < LEGACY_BUILD_CUTOFF { ClientGeneration::Legacy } else { ClientGeneration::Modern };
                return Self { build: Some(build), generation };
            }
        }
        Self::unknown()
    }

    /// The cho login body is `{username}\n{password_hash}\n{client_version}`.
    pub fn from_login_body(body: &[u8]) -> Self {
        let text = String::from_utf8_lossy(body);
        text.lines().nth(2).map(Self::from_version_line).unwrap_or_else(Self::unknown)
    }

    pub fn is_legacy(&self) -> bool {
        self.generation == ClientGeneration::Legacy
    }

    pub fn describe(&self) -> String {
        match self.build {
            Some(build) => {
                let label = match self.generation {
                    ClientGeneration::Legacy => "legacy",
                    ClientGeneration::Modern => "modern",
                    ClientGeneration::Unknown => "unknown",
                };
                format!("osu! build {} ({})", build, label)
            }
            None => "osu! build unknown".to_string(),
        }
    }
}

/// Reads up to `max` leading strings from a packet payload, returning them together with the
/// number of bytes still unread.
fn leading_strings(payload: &[u8], max: usize) -> (Vec<String>, usize) {
    let mut out = Vec::new();
    let mut pos = 0usize;

    for _ in 0..max {
        let mut reader = crate::packets::PacketReader::new(&payload[pos..]);
        // Only a string marker can start another string; anything else is the trailing
        // numeric field (the sender id) and ends the run.
        if !matches!(reader.read_u8().ok(), Some(0x0b) | Some(0x00)) {
            break;
        }
        let mut reader = crate::packets::PacketReader::new(&payload[pos..]);
        match reader.read_string() {
            Ok(s) => {
                let read = payload.len() - pos - reader.remaining();
                pos += read;
                out.push(s);
            }
            Err(_) => break,
        }
    }

    (out, payload.len() - pos)
}

fn looks_like_target(value: &str, player_name: &str) -> bool {
    let v = value.trim();
    if v.is_empty() {
        return false;
    }
    if v.starts_with('#') {
        return true; // a chat channel
    }
    if v.eq_ignore_ascii_case("BanchoBot") || v.eq_ignore_ascii_case("Tillerino") {
        return true; // a bot PM
    }
    // A username (a PM to another player).
    !v.eq_ignore_ascii_case(player_name)
        && v.len() <= 32
        && v.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '\''))
}

/// Extracts `(message, target)` from an `OSU_SEND_PUBLIC_MESSAGE` / `OSU_SEND_PRIVATE_MESSAGE`
/// payload.
///
/// Clients disagree on this layout, so both are supported:
///
/// * `message`, `target`, `sender_id` — what current builds send (the id is a 4-byte int on
///   older builds and an 8-byte long on newer ones).
/// * `sender`, `message`, `target`, `sender_id` — what some builds send, which is also the
///   layout the *server* uses for the outgoing variant of this packet. Reading this as the
///   three-field form yields the sender's name as the "message", which silently swallows every
///   command, so the two are told apart by looking at which field is a plausible target.
///
/// `player_name` is the logged-in user, which makes the sender-first layout unambiguous whenever
/// the first field matches it.
pub fn parse_chat_payload(payload: &[u8], player_name: &str) -> Option<(String, String)> {
    let (fields, tail) = leading_strings(payload, 3);

    // The trailing sender id is an int (4 bytes) or a long (8 bytes); the very old two-field
    // form has nothing at all. A partial id means the packet is malformed.
    if tail > 0 && tail < 4 {
        return None;
    }

    let sender_first = if fields.len() >= 3 && fields[0].eq_ignore_ascii_case(player_name) {
        true
    } else {
        fields.len() >= 3 && looks_like_target(&fields[1], player_name) && !looks_like_target(&fields[0], player_name)
    };

    if sender_first {
        return Some((fields[1].clone(), fields[2].clone()));
    }

    if fields.len() >= 2 {
        return Some((fields[0].clone(), fields[1].clone()));
    }

    None
}

/// Maps legacy/alias endpoint names onto the handler that serves them, so a client of any
/// vintage gets a 200 instead of a 404.
///
/// Only aliases whose semantics are identical are listed: a POST-only score submission is
/// deliberately *not* aliased onto another endpoint, because the two take different bodies.
pub fn canonical_web_path(path: &str) -> &str {
    match path {
        // Pre-`bancho_connect.php` name used by very old builds and osu!bancho clients.
        "/osu-login.php" => "/bancho_connect.php",
        // Older builds POST this after a beatmap change; it is a no-op for us either way.
        "/osu-mark-as-latest-map.php" => "/osu-markasread.php",
        // Legacy spelling of the beatmap info lookup.
        "/osu-beatmapinfo.php" => "/osu-getbeatmapinfo.php",
        // Session/token endpoints used by the web-side login flow.
        "/osu-token.php" => "/osu-session.php",
        other => other,
    }
}

/// Body returned for the web login endpoint. A private server has no website session, but the
/// client expects a JSON object and logs an error when the request fails outright, so a minimal
/// well-formed payload is preferable to a 404.
pub fn login_session_json(user_name: &str, user_id: i32) -> String {
    let escaped = user_name.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        "{{\"user\":{{\"id\":{},\"username\":\"{}\",\"profile\":{{\"country_code\":\"XX\"}}}},\"session\":\"ok\"}}",
        user_id, escaped
    )
}

/// `osu-getbeatmapinfo.php` / `difficulty-rating` expect a JSON array. An empty array is a valid
/// "no online data" answer; an empty body is not, because the client logs a parse failure.
pub const EMPTY_JSON_ARRAY: &str = "[]";

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packets::PacketReader;

    fn write_string(s: &str) -> Vec<u8> {
        let mut out = vec![0x0b];
        out.push(s.len() as u8);
        out.extend_from_slice(s.as_bytes());
        out
    }

    #[test]
    fn test_client_info_from_version_lines() {
        let legacy = ClientInfo::from_version_line("20220319.2");
        assert_eq!(legacy.build, Some(20220319));
        assert_eq!(legacy.generation, ClientGeneration::Legacy);
        assert!(legacy.is_legacy());

        let modern = ClientInfo::from_version_line("20260513.0");
        assert_eq!(modern.build, Some(20260513));
        assert_eq!(modern.generation, ClientGeneration::Modern);
        assert!(!modern.is_legacy());

        assert_eq!(ClientInfo::from_version_line("").build, None);
        assert_eq!(ClientInfo::from_version_line("stream").build, None);
    }

    #[test]
    fn test_client_info_from_login_body() {
        let body = b"Player\n0123456789abcdef0123456789abcdef\n20220319.2";
        assert!(ClientInfo::from_login_body(body).is_legacy());

        let modern = b"Player\nhash\n20260513";
        assert!(!ClientInfo::from_login_body(modern).is_legacy());

        // No version line at all (some clients only send two lines).
        assert_eq!(ClientInfo::from_login_body(b"Player\nhash").generation, ClientGeneration::Unknown);
    }

    #[test]
    fn test_parse_chat_payload_three_fields() {
        let mut payload = write_string("!help");
        payload.extend_from_slice(&write_string("#osu"));
        payload.extend_from_slice(&2i32.to_le_bytes()); // sender id (int form)

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "!help");
        assert_eq!(target, "#osu");
    }

    #[test]
    fn test_parse_chat_payload_accepts_long_sender_id() {
        let mut payload = write_string("hi");
        payload.extend_from_slice(&write_string("Someone"));
        payload.extend_from_slice(&1234567i64.to_le_bytes()); // sender id (long form)

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "hi");
        assert_eq!(target, "Someone");
    }

    #[test]
    fn test_parse_chat_payload_sender_first_layout() {
        // Some builds (and the server's own outgoing packet) put the sender first:
        // sender, message, target, sender_id.
        let mut payload = write_string("LegacyUser");
        payload.extend_from_slice(&write_string("!profile"));
        payload.extend_from_slice(&write_string("#osu"));
        payload.extend_from_slice(&2i32.to_le_bytes());

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "!profile");
        assert_eq!(target, "#osu");
    }

    #[test]
    fn test_parse_chat_payload_sender_first_pm_to_bot() {
        let mut payload = write_string("LegacyUser");
        payload.extend_from_slice(&write_string("/np"));
        payload.extend_from_slice(&write_string("Tillerino"));
        payload.extend_from_slice(&2i64.to_le_bytes());

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "/np");
        assert_eq!(target, "Tillerino");
    }

    #[test]
    fn test_parse_chat_payload_sender_first_layout_without_id() {
        let mut payload = write_string("LegacyUser");
        payload.extend_from_slice(&write_string("!stats"));
        payload.extend_from_slice(&write_string("BanchoBot"));

        let (message, target) = parse_chat_payload(&payload, "legacyuser").unwrap();
        assert_eq!(message, "!stats");
        assert_eq!(target, "BanchoBot");
    }

    #[test]
    fn test_parse_chat_payload_pm_target_is_a_username() {
        // Three-field form where the target is a plain username rather than a channel.
        let mut payload = write_string("!stats");
        payload.extend_from_slice(&write_string("SomeOtherPlayer"));
        payload.extend_from_slice(&2i32.to_le_bytes());

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "!stats");
        assert_eq!(target, "SomeOtherPlayer");
    }

    #[test]
    fn test_parse_chat_payload_two_field_legacy_form() {
        let mut payload = write_string("!stats");
        payload.extend_from_slice(&write_string("BanchoBot"));

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "!stats");
        assert_eq!(target, "BanchoBot");
    }

    #[test]
    fn test_parse_chat_payload_rejects_truncated_and_empty() {
        // Truncated: a partial sender id is left over.
        let mut truncated = write_string("hi");
        truncated.extend_from_slice(&write_string("#osu"));
        truncated.extend_from_slice(&[0x02, 0x00]);
        assert!(parse_chat_payload(&truncated, "LegacyUser").is_none());

        assert!(parse_chat_payload(&[], "LegacyUser").is_none());
        assert!(parse_chat_payload(&write_string("only one field"), "LegacyUser").is_none());
    }

    #[test]
    fn test_parse_chat_payload_empty_message_is_valid() {
        let mut payload = write_string("");
        payload.extend_from_slice(&write_string("#osu"));
        payload.extend_from_slice(&2i32.to_le_bytes());

        let (message, target) = parse_chat_payload(&payload, "LegacyUser").unwrap();
        assert_eq!(message, "");
        assert_eq!(target, "#osu");
    }

    #[test]
    fn test_reader_remaining_matches_payload_tail() {
        let mut payload = write_string("a");
        payload.extend_from_slice(&write_string("b"));
        let mut reader = PacketReader::new(&payload);
        reader.read_string().unwrap();
        reader.read_string().unwrap();
        assert_eq!(reader.remaining(), 0);
    }

    #[test]
    fn test_canonical_web_path_aliases() {
        assert_eq!(canonical_web_path("/osu-login.php"), "/bancho_connect.php");
        assert_eq!(canonical_web_path("/osu-mark-as-latest-map.php"), "/osu-markasread.php");
        assert_eq!(canonical_web_path("/osu-beatmapinfo.php"), "/osu-getbeatmapinfo.php");
        assert_eq!(canonical_web_path("/osu-token.php"), "/osu-session.php");
        // Untouched paths stay as they are.
        assert_eq!(canonical_web_path("/osu-search.php"), "/osu-search.php");
        assert_eq!(
            canonical_web_path("/osu-submit-modular-selector.php"),
            "/osu-submit-modular-selector.php"
        );
    }

    #[test]
    fn test_login_session_json_is_valid_json() {
        let body = login_session_json("Test\"User", 2);
        let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(parsed["user"]["id"], 2);
        assert_eq!(parsed["user"]["username"], "Test\"User");
    }
}
