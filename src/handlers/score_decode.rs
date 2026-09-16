use crate::server::multipart::parse_multipart;
use crate::types::score::Score;
use base64::Engine;
use sha2::{Digest, Sha256};
use simple_rijndael::impls::RijndaelCbc;
use simple_rijndael::paddings::Pkcs7Padding;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DecodedSubmission {
    pub score: Score,
    pub submission_checksum: String,
    pub passed: bool,
    pub date: String,
    pub osu_version: String,
    pub replay_frames: Option<Vec<u8>>,
    pub identity: Option<String>,
}

pub fn parse_submission(content_type: &str, body: &[u8]) -> Result<DecodedSubmission, String> {
    if body.len() > 16 * 1024 * 1024 {
        return Err("Payload exceeds 16MB limit".to_string());
    }

    let parts = parse_multipart(content_type, body)?;
    let mut fields: HashMap<String, Vec<u8>> = HashMap::new();
    let mut replay_frames: Option<Vec<u8>> = None;

    for part in parts {
        if part.name == "score" && part.filename.is_some() {
            if replay_frames.is_some() {
                return Err("Duplicate replay part".to_string());
            }
            replay_frames = Some(part.data);
        } else if part.name == "score" || part.name == "iv" || part.name == "osuver" {
            if fields.contains_key(&part.name) {
                return Err(format!("Duplicate field: {}", part.name));
            }
            fields.insert(part.name, part.data);
        }
    }

    let osuver_bytes = fields.get("osuver").ok_or("Missing osuver field")?;
    let osuver = String::from_utf8_lossy(osuver_bytes).trim().to_string();
    if osuver.len() < 8 || !osuver.chars().all(|c| c.is_ascii_digit()) {
        return Err("Invalid osuver format".to_string());
    }

    let iv_b64 = fields.get("iv").ok_or("Missing iv field")?;
    let score_b64 = fields.get("score").ok_or("Missing encrypted score field")?;

    let iv_str = String::from_utf8_lossy(iv_b64).trim().to_string();
    let score_str = String::from_utf8_lossy(score_b64).trim().to_string();

    let b64_engine = base64::engine::general_purpose::STANDARD;
    let iv = b64_engine.decode(iv_str.as_bytes()).map_err(|e| format!("Invalid base64 iv: {e}"))?;
    let encrypted = b64_engine.decode(score_str.as_bytes()).map_err(|e| format!("Invalid base64 score: {e}"))?;

    if iv.len() != 32 {
        return Err(format!("Invalid IV length: expected 32, got {}", iv.len()));
    }
    if encrypted.is_empty() || encrypted.len() % 32 != 0 || encrypted.len() > 65536 {
        return Err(format!("Invalid encrypted score length: {}", encrypted.len()));
    }

    let key = format!("osu!-scoreburgr---------{}", osuver).into_bytes();
    if key.len() != 32 {
        return Err(format!("Invalid key length derived from osuver: {}", key.len()));
    }

    let cipher = RijndaelCbc::<Pkcs7Padding>::new(&key, 32).map_err(|e| format!("Failed to create Rijndael cipher: {:?}", e))?;

    let decrypted = cipher.decrypt(&iv, encrypted).map_err(|e| format!("Failed to decrypt score payload: {:?}", e))?;

    let decrypted_str = String::from_utf8(decrypted).map_err(|e| format!("Decrypted payload is not valid UTF-8: {e}"))?;

    let data: Vec<&str> = decrypted_str.split(':').collect();
    if data.len() < 16 {
        return Err(format!("Insufficient fields in decrypted score: got {}", data.len()));
    }

    let beatmap_md5 = data[0].to_lowercase();
    let player_name = data[1].trim().to_string();
    let submission_checksum = data[2].to_string();

    let n300: i32 = data[3].parse().map_err(|_| "Invalid n300")?;
    let n100: i32 = data[4].parse().map_err(|_| "Invalid n100")?;
    let n50: i32 = data[5].parse().map_err(|_| "Invalid n50")?;
    let ngeki: i32 = data[6].parse().map_err(|_| "Invalid ngeki")?;
    let nkatu: i32 = data[7].parse().map_err(|_| "Invalid nkatu")?;
    let nmiss: i32 = data[8].parse().map_err(|_| "Invalid nmiss")?;

    let total_score: i64 = data[9].parse().map_err(|_| "Invalid score")?;
    let max_combo: i32 = data[10].parse().map_err(|_| "Invalid max_combo")?;
    let perfect = data[11].eq_ignore_ascii_case("true");
    let _grade = data[12];
    let mods: u32 = data[13].parse().map_err(|_| "Invalid mods")?;
    let passed = data[14].eq_ignore_ascii_case("true");
    let mode: i32 = data[15].parse().map_err(|_| "Invalid mode")?;
    let date = data.get(16).unwrap_or(&"").to_string();
    let osu_version = data.get(17).unwrap_or(&"").to_string();

    let now_ts = chrono::Utc::now().timestamp();

    let b64_frames = replay_frames.as_ref().map(|f| b64_engine.encode(f));

    let identity = if let Some(ref frames) = replay_frames {
        let repr = format!(
            "('{}', '{}', {}, {}, {}, {}, {}, {}, {}, {}, {})",
            beatmap_md5, player_name, mods, total_score, max_combo, n300, n100, n50, ngeki, nkatu, nmiss
        );
        let mut hasher = Sha256::new();
        hasher.update(repr.as_bytes());
        hasher.update(frames);
        let hash_result = hasher.finalize();
        let hex_str: String = hash_result.iter().map(|b| format!("{:02x}", b)).collect();
        Some(hex_str)
    } else {
        None
    };

    let score = Score {
        scoreid: None,
        md5: beatmap_md5.clone(),
        name: player_name.clone(),
        score: total_score,
        max_combo,
        mods,
        n300,
        n100,
        n50,
        ngeki,
        nkatu,
        nmiss,
        time: now_ts,
        perfect,
        pp: None,
        acc: None,
        mode,
        mods_str: None,
        replay_md5: None,
        replay_frames: b64_frames,
        additional_mods: None,
        submission_checksum: Some(submission_checksum.clone()),
        submission_identity: identity.clone(),
    };

    Ok(DecodedSubmission { score, submission_checksum, passed, date, osu_version, replay_frames, identity })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rijndael_score_roundtrip() {
        let osuver = "20260912";
        let key = format!("osu!-scoreburgr---------{}", osuver).into_bytes();
        let iv = vec![7u8; 32];

        let plaintext = format!("{}:{}:{}:99:1:0:0:0:0:100000:100:True:S:0:True:0:260912120000:20260912", "a".repeat(32), "Alice", "b".repeat(32));

        let cipher = RijndaelCbc::<Pkcs7Padding>::new(&key, 32).unwrap();
        let encrypted = cipher.encrypt(&iv, plaintext.into_bytes()).unwrap();

        let b64 = base64::engine::general_purpose::STANDARD;
        let iv_b64 = b64.encode(&iv);
        let score_b64 = b64.encode(&encrypted);

        let body = format!(
            "--boundary\r\nContent-Disposition: form-data; name=\"osuver\"\r\n\r\n{}\r\n\
--boundary\r\nContent-Disposition: form-data; name=\"iv\"\r\n\r\n{}\r\n\
--boundary\r\nContent-Disposition: form-data; name=\"score\"\r\n\r\n{}\r\n\
--boundary\r\nContent-Disposition: form-data; name=\"score\"; filename=\"replay.osr\"\r\n\r\nreplaydata\r\n\
--boundary--\r\n",
            osuver, iv_b64, score_b64
        );

        let content_type = "multipart/form-data; boundary=boundary";
        let sub = parse_submission(content_type, body.as_bytes()).unwrap();

        assert_eq!(sub.score.name, "Alice");
        assert_eq!(sub.score.score, 100000);
        assert_eq!(sub.score.max_combo, 100);
        assert_eq!(sub.score.n300, 99);
        assert_eq!(sub.score.n100, 1);
        assert_eq!(sub.score.nmiss, 0);
        assert!(sub.passed);
        assert_eq!(sub.replay_frames.as_deref(), Some(b"replaydata".as_slice()));
        assert!(sub.identity.is_some());
    }
}
