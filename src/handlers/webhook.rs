use crate::types::beatmap::Beatmap;
use crate::types::score::Score;
use crate::utils;
use chrono::Utc;
use reqwest::Client as HttpClient;
use serde_json::json;

pub fn format_number(val: i64) -> String {
    let s = val.to_string();
    let bytes = s.as_bytes();
    let mut result = String::with_capacity(s.len() + (s.len() / 3));
    let offset = bytes.len() % 3;

    for (i, &b) in bytes.iter().enumerate() {
        if i > 0 && (i % 3 == offset || (offset == 0 && i % 3 == 0)) {
            result.push(',');
        }
        result.push(b as char);
    }
    result
}

pub fn grade_color(grade: &str) -> u32 {
    match grade {
        "XH" | "SSH" => 0xe0e0e0,
        "SS" | "X" => 0xd6c253,
        "SH" => 0x80d8ff,
        "S" => 0x00d26a,
        "A" => 0x22c55e,
        "B" => 0x3b82f6,
        "C" => 0xa855f7,
        "D" | "F" => 0xef4444,
        _ => 0x71717a,
    }
}

pub fn build_score_webhook_payload(
    player_name: &str,
    score: &Score,
    bmap: &Beatmap,
    rank_after: Option<i32>,
    player_rank: i32,
    player_pp: f64,
) -> serde_json::Value {
    let grade = utils::get_grade(
        score.mode as u8,
        score.n300,
        score.n100,
        score.n50,
        score.ngeki,
        score.nkatu,
        score.nmiss,
        score.mods,
        score.acc.unwrap_or(0.0),
    );

    let mods_str = score.mods_str.as_deref().unwrap_or("NM");
    let formatted_mods = if mods_str == "NM" { String::new() } else { format!(" +{}", mods_str) };

    let beatmap_url = if bmap.beatmap_id > 0 {
        Some(format!("https://osu.ppy.sh/b/{}", bmap.beatmap_id))
    } else if bmap.beatmapset_id > 0 {
        Some(format!("https://osu.ppy.sh/s/{}", bmap.beatmapset_id))
    } else {
        None
    };

    let thumbnail_url = if bmap.beatmapset_id > 0 {
        format!("https://b.ppy.sh/thumb/{}l.jpg", bmap.beatmapset_id)
    } else {
        "https://osu.ppy.sh/images/layout/medals/all.png".to_string()
    };

    let is_fc = score.nmiss == 0 && (score.perfect || (bmap.max_combo > 0 && score.max_combo >= bmap.max_combo - 5));
    let combo_str = if bmap.max_combo > 0 {
        format!("{}x / {}x{}", score.max_combo, bmap.max_combo, if is_fc { " [FC]" } else { "" })
    } else {
        format!("{}x{}", score.max_combo, if is_fc { " [FC]" } else { "" })
    };

    // Legacy Bancho protocol maps mode-specific judgement counts into generic score fields:
    // Taiko: 300 = GREAT, 100 = GOOD
    // Catch: 300 = Fruits, 100 = Drops, 50 = Droplets
    // Mania: ngeki = MAX (300g), 300 = 300, nkatu = 200, 100 = 100, 50 = 50
    let hits_str = match score.mode {
        1 => format!("{}/{} (Miss: {})", score.n300, score.n100, score.nmiss),
        2 => format!("{}/{}/{} (Miss: {})", score.n300, score.n100, score.n50, score.nmiss),
        3 => format!("{}/{}/{}/{}/{} (Miss: {})", score.ngeki, score.n300, score.nkatu, score.n100, score.n50, score.nmiss),
        _ => format!("{}/{}/{} (Miss: {})", score.n300, score.n100, score.n50, score.nmiss),
    };

    let rank_note = match rank_after {
        Some(1) => " • #1 on Map!",
        Some(r) if r <= 50 => " • Local Top 50",
        _ => "",
    };

    let mut fields = vec![
        json!({ "name": "Grade", "value": format!("`{}`", grade), "inline": true }),
        json!({ "name": "Score", "value": format_number(score.score), "inline": true }),
        json!({ "name": "Accuracy", "value": format!("{:.2}%", score.acc.unwrap_or(0.0)), "inline": true }),
        json!({ "name": "Combo", "value": combo_str, "inline": true }),
        json!({ "name": "Hits", "value": hits_str, "inline": true }),
    ];

    if let Some(pp) = score.pp {
        if pp > 0.0 {
            fields.push(json!({ "name": "PP", "value": format!("**{:.2}pp**", pp), "inline": true }));
        }
    }

    let mode_display = match score.mode {
        1 => "osu!taiko",
        2 => "osu!catch",
        3 => "osu!mania",
        _ => "osu!",
    };

    let footer_text = if player_rank > 0 {
        format!("{} • #{} Global ({:.0}pp)", mode_display, format_number(player_rank as i64), player_pp)
    } else {
        mode_display.to_string()
    };

    json!({
        "username": "osu-echo",
        "avatar_url": "https://a.ppy.sh/2",
        "embeds": [{
            "title": format!("{} - {} [{}]{}", bmap.artist, bmap.title, bmap.version, formatted_mods),
            "url": beatmap_url,
            "color": grade_color(grade),
            "author": {
                "name": format!("{}{}", player_name, rank_note),
                "icon_url": format!("https://a.ppy.sh/{}", player_name)
            },
            "thumbnail": {
                "url": thumbnail_url
            },
            "fields": fields,
            "footer": {
                "text": footer_text
            },
            "timestamp": Utc::now().to_rfc3339()
        }]
    })
}

#[allow(clippy::too_many_arguments)]
pub fn send_score_webhook(
    http: HttpClient,
    webhook_url: String,
    player_name: String,
    score: Score,
    bmap: Beatmap,
    rank_after: Option<i32>,
    player_rank: i32,
    player_pp: f64,
) {
    tokio::spawn(async move {
        let payload = build_score_webhook_payload(&player_name, &score, &bmap, rank_after, player_rank, player_pp);

        match http
            .post(&webhook_url)
            .header("Content-Type", "application/json")
            .json(&payload)
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await
        {
            Ok(resp) if resp.status().is_success() => {
                crate::logger::info("Discord webhook score posted successfully.");
            }
            Ok(resp) => {
                let status = resp.status();
                let text = resp.text().await.unwrap_or_default();
                crate::logger::warn(&format!("Discord webhook returned error status {}: {}", status, text));
            }
            Err(e) => {
                crate::logger::warn(&format!("Failed to post score to Discord webhook: {}", e));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_number() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1000), "1,000");
        assert_eq!(format_number(12345), "12,345");
        assert_eq!(format_number(123456), "123,456");
        assert_eq!(format_number(12345678), "12,345,678");
    }

    #[test]
    fn test_grade_color() {
        assert_eq!(grade_color("SS"), 0xd6c253);
        assert_eq!(grade_color("SSH"), 0xe0e0e0);
        assert_eq!(grade_color("S"), 0x00d26a);
        assert_eq!(grade_color("A"), 0x22c55e);
        assert_eq!(grade_color("D"), 0xef4444);
    }

    #[test]
    fn test_build_score_webhook_payload() {
        let mut bmap = Beatmap::blank();
        bmap.beatmap_id = 12345;
        bmap.beatmapset_id = 67890;
        bmap.artist = "Artist".to_string();
        bmap.title = "Song Title".to_string();
        bmap.version = "Expert".to_string();
        bmap.max_combo = 1000;

        let score = Score {
            scoreid: Some(1),
            md5: "abc".to_string(),
            name: "Player1".to_string(),
            score: 12345678,
            max_combo: 1000,
            mods: 0,
            n300: 500,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            time: 0,
            perfect: true,
            pp: Some(300.5),
            acc: Some(100.0),
            mode: 0,
            mods_str: Some("NM".to_string()),
            replay_md5: None,
            replay_frames: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };

        let payload = build_score_webhook_payload("Player1", &score, &bmap, Some(1), 100, 5000.0);
        let embeds = payload.get("embeds").and_then(|v| v.as_array()).unwrap();
        assert_eq!(embeds.len(), 1);

        let embed = &embeds[0];
        assert_eq!(embed["title"], "Artist - Song Title [Expert]");
        assert_eq!(embed["url"], "https://osu.ppy.sh/b/12345");
        assert!(embed["author"]["name"].as_str().unwrap().contains("#1 on Map!"));
    }
}
