use crate::types::beatmap::Beatmap;
use crate::types::config::Config;
use crate::types::score::Score;
use rusqlite::{params, Connection, Result as SqlResult};

pub fn init_db(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(
        "
        PRAGMA journal_mode = WAL;
        PRAGMA busy_timeout = 5000;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS config (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS profiles (
            name TEXT PRIMARY KEY,
            pp REAL NOT NULL DEFAULT 0,
            acc REAL NOT NULL DEFAULT 0,
            playcount INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS scores (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            player_name TEXT NOT NULL,
            mode INTEGER NOT NULL DEFAULT 0,
            md5 TEXT NOT NULL,
            n300 INTEGER NOT NULL DEFAULT 0,
            n100 INTEGER NOT NULL DEFAULT 0,
            n50 INTEGER NOT NULL DEFAULT 0,
            ngeki INTEGER NOT NULL DEFAULT 0,
            nkatu INTEGER NOT NULL DEFAULT 0,
            nmiss INTEGER NOT NULL DEFAULT 0,
            score INTEGER NOT NULL DEFAULT 0,
            max_combo INTEGER NOT NULL DEFAULT 0,
            perfect INTEGER NOT NULL DEFAULT 0,
            mods INTEGER NOT NULL DEFAULT 0,
            time INTEGER NOT NULL DEFAULT 0,
            acc REAL,
            pp REAL,
            replay_md5 TEXT,
            replay_frames TEXT,
            mods_str TEXT,
            bmap_status TEXT NOT NULL DEFAULT 'ranked',
            submission_checksum TEXT,
            submission_identity TEXT,
            FOREIGN KEY (player_name) REFERENCES profiles(name)
        );

        CREATE INDEX IF NOT EXISTS idx_scores_player ON scores(player_name);
        CREATE INDEX IF NOT EXISTS idx_scores_md5 ON scores(md5);
        CREATE INDEX IF NOT EXISTS idx_scores_status ON scores(bmap_status);
        CREATE INDEX IF NOT EXISTS idx_scores_replay_md5 ON scores(replay_md5);

        CREATE TABLE IF NOT EXISTS beatmaps (
            file_md5 TEXT PRIMARY KEY,
            beatmap_id INTEGER NOT NULL,
            beatmapset_id INTEGER NOT NULL,
            approved INTEGER NOT NULL DEFAULT 0,
            total_length INTEGER NOT NULL DEFAULT 0,
            hit_length INTEGER NOT NULL DEFAULT 0,
            version TEXT NOT NULL DEFAULT '',
            diff_size REAL NOT NULL DEFAULT 0,
            diff_overall REAL NOT NULL DEFAULT 0,
            diff_approach REAL NOT NULL DEFAULT 0,
            diff_drain REAL NOT NULL DEFAULT 0,
            mode INTEGER NOT NULL DEFAULT 0,
            count_normal INTEGER NOT NULL DEFAULT 0,
            count_slider INTEGER NOT NULL DEFAULT 0,
            count_spinner INTEGER NOT NULL DEFAULT 0,
            submit_date TEXT NOT NULL DEFAULT '',
            approved_date TEXT,
            last_update TEXT NOT NULL DEFAULT '',
            artist TEXT NOT NULL DEFAULT '',
            artist_unicode TEXT,
            title TEXT NOT NULL DEFAULT '',
            title_unicode TEXT,
            creator TEXT NOT NULL DEFAULT '',
            creator_id INTEGER NOT NULL DEFAULT 0,
            bpm REAL NOT NULL DEFAULT 0,
            source TEXT NOT NULL DEFAULT '',
            tags TEXT NOT NULL DEFAULT '',
            genre_id INTEGER NOT NULL DEFAULT 0,
            language_id INTEGER NOT NULL DEFAULT 0,
            favourite_count INTEGER NOT NULL DEFAULT 0,
            rating REAL NOT NULL DEFAULT 0,
            storyboard INTEGER NOT NULL DEFAULT 0,
            video INTEGER NOT NULL DEFAULT 0,
            download_unavailable INTEGER NOT NULL DEFAULT 0,
            audio_unavailable INTEGER NOT NULL DEFAULT 0,
            playcount INTEGER NOT NULL DEFAULT 0,
            passcount INTEGER NOT NULL DEFAULT 0,
            max_combo INTEGER NOT NULL DEFAULT 0,
            diff_aim REAL NOT NULL DEFAULT 0,
            diff_speed REAL NOT NULL DEFAULT 0,
            difficultyrating REAL NOT NULL DEFAULT 0,
            file_content TEXT
        );

        CREATE INDEX IF NOT EXISTS idx_beatmaps_id ON beatmaps(beatmap_id);

        CREATE TABLE IF NOT EXISTS avatars (
            player_name TEXT PRIMARY KEY,
            avatar_url TEXT
        );

        CREATE TABLE IF NOT EXISTS friends (
            player_name TEXT NOT NULL,
            friend_id INTEGER NOT NULL,
            friend_name TEXT NOT NULL DEFAULT '',
            rank INTEGER NOT NULL DEFAULT 1,
            pp INTEGER NOT NULL DEFAULT 0,
            acc REAL NOT NULL DEFAULT 0.0,
            country INTEGER NOT NULL DEFAULT 0,
            ranked_score INTEGER NOT NULL DEFAULT 0,
            total_score INTEGER NOT NULL DEFAULT 0,
            playcount INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (player_name, friend_id)
        );

        CREATE INDEX IF NOT EXISTS idx_friends_player ON friends(player_name);
    ",
    )?;

    // Migrations for existing DB files
    let _ = conn.execute("ALTER TABLE scores ADD COLUMN submission_checksum TEXT", []);
    let _ = conn.execute("ALTER TABLE scores ADD COLUMN submission_identity TEXT", []);
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_scores_checksum ON scores(submission_checksum)", []);
    let _ = conn.execute("CREATE INDEX IF NOT EXISTS idx_scores_identity ON scores(submission_identity)", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN rank INTEGER NOT NULL DEFAULT 1", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN pp INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN acc REAL NOT NULL DEFAULT 0.0", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN country INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN ranked_score INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN total_score INTEGER NOT NULL DEFAULT 0", []);
    let _ = conn.execute("ALTER TABLE friends ADD COLUMN playcount INTEGER NOT NULL DEFAULT 0", []);

    let _ = conn.execute("DELETE FROM friends WHERE friend_id <= 2 OR friend_id = 2070907", []);
    let _ = conn.execute("DELETE FROM profiles WHERE name = 'Friend 2'", []);
    let _ = conn.execute("DELETE FROM avatars WHERE player_name = 'Friend 2'", []);

    Ok(())
}

pub fn save_config(conn: &Connection, config: &Config) -> SqlResult<()> {
    let json = serde_json::to_string(config).unwrap_or_default();
    conn.execute("INSERT OR REPLACE INTO config (key, value) VALUES ('main', ?1)", params![json])?;
    Ok(())
}

pub fn load_config(conn: &Connection) -> SqlResult<Option<Config>> {
    let mut stmt = conn.prepare("SELECT value FROM config WHERE key = 'main'")?;
    let result = stmt.query_row([], |row| {
        let json: String = row.get(0)?;
        Ok(json)
    });

    match result {
        Ok(json) => Ok(serde_json::from_str(&json).ok()),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn ensure_profile(conn: &Connection, name: &str) -> SqlResult<()> {
    conn.execute("INSERT OR IGNORE INTO profiles (name, pp, acc, playcount) VALUES (?1, 0, 0, 0)", params![name])?;
    Ok(())
}

pub fn get_playcount(conn: &Connection, name: &str) -> SqlResult<i32> {
    let mut stmt = conn.prepare("SELECT playcount FROM profiles WHERE name = ?1")?;
    stmt.query_row(params![name], |row| row.get(0)).or(Ok(0))
}

pub fn increment_playcount(conn: &Connection, name: &str) -> SqlResult<()> {
    conn.execute("UPDATE profiles SET playcount = playcount + 1 WHERE name = ?1", params![name])?;
    Ok(())
}

pub fn update_profile_stats(conn: &Connection, name: &str, pp: f64, acc: f64) -> SqlResult<()> {
    conn.execute("UPDATE profiles SET pp = ?1, acc = ?2 WHERE name = ?3", params![pp, acc, name])?;
    Ok(())
}

pub fn profile_exists(conn: &Connection, name: &str) -> SqlResult<bool> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM profiles WHERE name = ?1")?;
    let count: i32 = stmt.query_row(params![name], |row| row.get(0))?;
    Ok(count > 0)
}

pub fn wipe_profile(conn: &Connection, name: &str) -> SqlResult<()> {
    conn.execute("DELETE FROM scores WHERE player_name = ?1", params![name])?;
    conn.execute("UPDATE profiles SET pp = 0, acc = 0, playcount = 0 WHERE name = ?1", params![name])?;
    Ok(())
}

pub fn insert_score(conn: &Connection, score: &Score, bmap_status: &str) -> SqlResult<i64> {
    conn.execute(
        "INSERT INTO scores (
            player_name, mode, md5, n300, n100, n50, ngeki, nkatu, nmiss,
            score, max_combo, perfect, mods, time, acc, pp,
            replay_md5, replay_frames, mods_str, bmap_status,
            submission_checksum, submission_identity
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)",
        params![
            score.name,
            score.mode,
            score.md5,
            score.n300,
            score.n100,
            score.n50,
            score.ngeki,
            score.nkatu,
            score.nmiss,
            score.score,
            score.max_combo,
            score.perfect as i32,
            score.mods,
            score.time,
            score.acc,
            score.pp,
            score.replay_md5,
            score.replay_frames,
            score.mods_str,
            bmap_status,
            score.submission_checksum,
            score.submission_identity,
        ],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn replay_md5_exists(conn: &Connection, replay_md5: &str) -> SqlResult<bool> {
    let mut stmt = conn.prepare("SELECT COUNT(*) FROM scores WHERE replay_md5 = ?1")?;
    let count: i32 = stmt.query_row(params![replay_md5], |row| row.get(0))?;
    Ok(count > 0)
}

pub fn check_duplicate_score(conn: &Connection, checksum: Option<&str>, replay_md5: Option<&str>, identity: Option<&str>) -> SqlResult<Option<i64>> {
    if let Some(c) = checksum {
        if !c.is_empty() {
            let mut stmt = conn.prepare("SELECT id FROM scores WHERE submission_checksum = ?1 LIMIT 1")?;
            if let Ok(id) = stmt.query_row(params![c], |r| r.get(0)) {
                return Ok(Some(id));
            }
        }
    }
    if let Some(r) = replay_md5 {
        if !r.is_empty() {
            let mut stmt = conn.prepare("SELECT id FROM scores WHERE replay_md5 = ?1 LIMIT 1")?;
            if let Ok(id) = stmt.query_row(params![r], |r| r.get(0)) {
                return Ok(Some(id));
            }
        }
    }
    if let Some(i) = identity {
        if !i.is_empty() {
            let mut stmt = conn.prepare("SELECT id FROM scores WHERE submission_identity = ?1 LIMIT 1")?;
            if let Ok(id) = stmt.query_row(params![i], |r| r.get(0)) {
                return Ok(Some(id));
            }
        }
    }
    Ok(None)
}

pub fn update_beatmap_file_content(conn: &Connection, md5: &str, content: &str) -> SqlResult<()> {
    conn.execute("UPDATE beatmaps SET file_content = ?1 WHERE file_md5 = ?2", params![content, md5])?;
    Ok(())
}

pub fn get_ranked_scores(conn: &Connection, name: &str) -> SqlResult<Vec<Score>> {
    let mut stmt = conn.prepare(
        "SELECT id, mode, md5, n300, n100, n50, ngeki, nkatu, nmiss,
                score, max_combo, perfect, mods, time, acc, pp,
                replay_md5, replay_frames, mods_str, submission_checksum, submission_identity
         FROM scores
         WHERE player_name = ?1
           AND (bmap_status IN ('ranked', 'approved')
                OR md5 IN (SELECT file_md5 FROM beatmaps WHERE approved IN (1, 2)))
         ORDER BY pp DESC",
    )?;

    let scores = stmt
        .query_map(params![name], |row| {
            Ok(Score {
                mode: row.get(1)?,
                md5: row.get(2)?,
                name: name.to_string(),
                n300: row.get(3)?,
                n100: row.get(4)?,
                n50: row.get(5)?,
                ngeki: row.get(6)?,
                nkatu: row.get(7)?,
                nmiss: row.get(8)?,
                score: row.get(9)?,
                max_combo: row.get(10)?,
                perfect: row.get::<_, i32>(11)? != 0,
                mods: row.get::<_, i64>(12)? as u32,
                time: row.get(13)?,
                acc: row.get(14)?,
                pp: row.get(15)?,
                replay_md5: row.get(16)?,
                replay_frames: row.get(17)?,
                mods_str: row.get(18)?,
                scoreid: Some(row.get::<_, i64>(0)?),
                additional_mods: None,
                submission_checksum: row.get(19).ok(),
                submission_identity: row.get(20).ok(),
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(scores)
}

pub fn get_all_scores(conn: &Connection, name: &str) -> SqlResult<Vec<Score>> {
    let mut stmt = conn.prepare(
        "SELECT id, mode, md5, n300, n100, n50, ngeki, nkatu, nmiss,
                score, max_combo, perfect, mods, time, acc, pp,
                replay_md5, replay_frames, mods_str
         FROM scores
         WHERE player_name = ?1
         ORDER BY time DESC",
    )?;

    let scores = stmt
        .query_map(params![name], |row| {
            Ok(Score {
                mode: row.get(1)?,
                md5: row.get(2)?,
                name: name.to_string(),
                n300: row.get(3)?,
                n100: row.get(4)?,
                n50: row.get(5)?,
                ngeki: row.get(6)?,
                nkatu: row.get(7)?,
                nmiss: row.get(8)?,
                score: row.get(9)?,
                max_combo: row.get(10)?,
                perfect: row.get::<_, i32>(11)? != 0,
                mods: row.get::<_, i64>(12)? as u32,
                time: row.get(13)?,
                acc: row.get(14)?,
                pp: row.get(15)?,
                replay_md5: row.get(16)?,
                replay_frames: row.get(17)?,
                mods_str: row.get(18)?,
                scoreid: Some(row.get::<_, i64>(0)?),
                additional_mods: None,
                submission_checksum: None,
                submission_identity: None,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(scores)
}

pub fn get_scores_on_map(conn: &Connection, name: &str, md5: &str, mode: i32) -> SqlResult<Vec<Score>> {
    let mut stmt = conn.prepare(
        "SELECT id, mode, md5, n300, n100, n50, ngeki, nkatu, nmiss,
                score, max_combo, perfect, mods, time, acc, pp,
                replay_md5, replay_frames, mods_str
         FROM scores
         WHERE player_name = ?1 AND md5 = ?2 AND mode = ?3
         ORDER BY pp DESC",
    )?;

    let scores = stmt
        .query_map(params![name, md5, mode], |row| {
            Ok(Score {
                mode: row.get(1)?,
                md5: row.get(2)?,
                name: name.to_string(),
                n300: row.get(3)?,
                n100: row.get(4)?,
                n50: row.get(5)?,
                ngeki: row.get(6)?,
                nkatu: row.get(7)?,
                nmiss: row.get(8)?,
                score: row.get(9)?,
                max_combo: row.get(10)?,
                perfect: row.get::<_, i32>(11)? != 0,
                mods: row.get::<_, i64>(12)? as u32,
                time: row.get(13)?,
                acc: row.get(14)?,
                pp: row.get(15)?,
                replay_md5: row.get(16)?,
                replay_frames: row.get(17)?,
                mods_str: row.get(18)?,
                scoreid: Some(row.get::<_, i64>(0)?),
                additional_mods: None,
                submission_checksum: None,
                submission_identity: None,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(scores)
}

pub fn get_max_combo_for_player(conn: &Connection, name: &str, mode: i32) -> SqlResult<i32> {
    let mut stmt = conn.prepare("SELECT COALESCE(MAX(max_combo), 0) FROM scores WHERE player_name = ?1 AND mode = ?2")?;
    let max_combo: i32 = stmt.query_row(params![name, mode], |row| row.get(0))?;
    Ok(max_combo)
}

pub fn get_all_scores_on_map(conn: &Connection, md5: &str, mode: i32) -> SqlResult<Vec<Score>> {
    let mut stmt = conn.prepare(
        "SELECT id, mode, md5, n300, n100, n50, ngeki, nkatu, nmiss,
                score, max_combo, perfect, mods, time, acc, pp,
                replay_md5, replay_frames, mods_str, player_name
         FROM scores
         WHERE md5 = ?1 AND mode = ?2
         ORDER BY pp DESC",
    )?;

    let scores = stmt
        .query_map(params![md5, mode], |row| {
            Ok(Score {
                mode: row.get(1)?,
                md5: row.get(2)?,
                name: row.get(19)?,
                n300: row.get(3)?,
                n100: row.get(4)?,
                n50: row.get(5)?,
                ngeki: row.get(6)?,
                nkatu: row.get(7)?,
                nmiss: row.get(8)?,
                score: row.get(9)?,
                max_combo: row.get(10)?,
                perfect: row.get::<_, i32>(11)? != 0,
                mods: row.get::<_, i64>(12)? as u32,
                time: row.get(13)?,
                acc: row.get(14)?,
                pp: row.get(15)?,
                replay_md5: row.get(16)?,
                replay_frames: row.get(17)?,
                mods_str: row.get(18)?,
                scoreid: Some(row.get::<_, i64>(0)?),
                additional_mods: None,
                submission_checksum: None,
                submission_identity: None,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(scores)
}

pub fn get_score_by_id(conn: &Connection, id: i64) -> SqlResult<Option<Score>> {
    let mut stmt = conn.prepare(
        "SELECT id, player_name, mode, md5, n300, n100, n50, ngeki, nkatu, nmiss,
                score, max_combo, perfect, mods, time, acc, pp,
                replay_md5, replay_frames, mods_str
         FROM scores WHERE id = ?1",
    )?;

    let result = stmt.query_row(params![id], |row| {
        Ok(Score {
            mode: row.get(2)?,
            md5: row.get(3)?,
            name: row.get::<_, String>(1)?,
            n300: row.get(4)?,
            n100: row.get(5)?,
            n50: row.get(6)?,
            ngeki: row.get(7)?,
            nkatu: row.get(8)?,
            nmiss: row.get(9)?,
            score: row.get(10)?,
            max_combo: row.get(11)?,
            perfect: row.get::<_, i32>(12)? != 0,
            mods: row.get::<_, i64>(13)? as u32,
            time: row.get(14)?,
            acc: row.get(15)?,
            pp: row.get(16)?,
            replay_md5: row.get(17)?,
            replay_frames: row.get(18)?,
            mods_str: row.get(19)?,
            scoreid: Some(row.get::<_, i64>(0)?),
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        })
    });

    match result {
        Ok(s) => Ok(Some(s)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn get_beatmap_by_md5(conn: &Connection, md5: &str) -> SqlResult<Option<Beatmap>> {
    let mut stmt = conn.prepare("SELECT * FROM beatmaps WHERE file_md5 = ?1")?;

    let result = stmt.query_row(params![md5], |row| Ok(row_to_beatmap(row)));

    match result {
        Ok(b) => Ok(Some(b)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn get_beatmap_by_id(conn: &Connection, id: i64) -> SqlResult<Option<Beatmap>> {
    let mut stmt = conn.prepare("SELECT * FROM beatmaps WHERE beatmap_id = ?1")?;

    let result = stmt.query_row(params![id], |row| Ok(row_to_beatmap(row)));

    match result {
        Ok(b) => Ok(Some(b)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn get_candidate_beatmaps(conn: &Connection, mode: i32) -> SqlResult<Vec<Beatmap>> {
    let mut stmt = conn.prepare("SELECT * FROM beatmaps WHERE mode = ?1 ORDER BY playcount DESC, difficultyrating ASC")?;
    let mut maps: Vec<Beatmap> = stmt
        .query_map(params![mode], |row| Ok(row_to_beatmap(row)))?
        .filter_map(|r| r.ok())
        .collect();

    // Fallback: if no maps match the specific mode, retrieve any available beatmaps
    if maps.is_empty() {
        let mut fallback_stmt = conn.prepare("SELECT * FROM beatmaps ORDER BY playcount DESC, difficultyrating ASC")?;
        maps = fallback_stmt
            .query_map([], |row| Ok(row_to_beatmap(row)))?
            .filter_map(|r| r.ok())
            .collect();
    }

    Ok(maps)
}

pub fn insert_beatmap(conn: &Connection, bmap: &Beatmap) -> SqlResult<()> {
    conn.execute(
        "INSERT OR REPLACE INTO beatmaps (
            file_md5, beatmap_id, beatmapset_id, approved, total_length, hit_length,
            version, diff_size, diff_overall, diff_approach, diff_drain, mode,
            count_normal, count_slider, count_spinner, submit_date, approved_date,
            last_update, artist, artist_unicode, title, title_unicode, creator,
            creator_id, bpm, source, tags, genre_id, language_id, favourite_count,
            rating, storyboard, video, download_unavailable, audio_unavailable,
            playcount, passcount, max_combo, diff_aim, diff_speed, difficultyrating,
            file_content
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23,
            ?24, ?25, ?26, ?27, ?28, ?29, ?30, ?31, ?32, ?33, ?34,
            ?35, ?36, ?37, ?38, ?39, ?40, ?41, ?42
        )",
        params![
            bmap.file_md5,
            bmap.beatmap_id,
            bmap.beatmapset_id,
            bmap.approved,
            bmap.total_length,
            bmap.hit_length,
            bmap.version,
            bmap.diff_size,
            bmap.diff_overall,
            bmap.diff_approach,
            bmap.diff_drain,
            bmap.mode,
            bmap.count_normal,
            bmap.count_slider,
            bmap.count_spinner,
            bmap.submit_date,
            bmap.approved_date,
            bmap.last_update,
            bmap.artist,
            bmap.artist_unicode,
            bmap.title,
            bmap.title_unicode,
            bmap.creator,
            bmap.creator_id,
            bmap.bpm,
            bmap.source,
            bmap.tags,
            bmap.genre_id,
            bmap.language_id,
            bmap.favourite_count,
            bmap.rating,
            bmap.storyboard,
            bmap.video,
            bmap.download_unavailable,
            bmap.audio_unavailable,
            bmap.playcount,
            bmap.passcount,
            bmap.max_combo,
            bmap.diff_aim,
            bmap.diff_speed,
            bmap.difficultyrating,
            bmap.file_content,
        ],
    )?;
    Ok(())
}

fn row_to_beatmap(row: &rusqlite::Row) -> Beatmap {
    Beatmap {
        file_md5: row.get(0).unwrap_or_default(),
        beatmap_id: row.get(1).unwrap_or_default(),
        beatmapset_id: row.get(2).unwrap_or_default(),
        approved: row.get(3).unwrap_or_default(),
        total_length: row.get(4).unwrap_or_default(),
        hit_length: row.get(5).unwrap_or_default(),
        version: row.get(6).unwrap_or_default(),
        diff_size: row.get(7).unwrap_or_default(),
        diff_overall: row.get(8).unwrap_or_default(),
        diff_approach: row.get(9).unwrap_or_default(),
        diff_drain: row.get(10).unwrap_or_default(),
        mode: row.get(11).unwrap_or_default(),
        count_normal: row.get(12).unwrap_or_default(),
        count_slider: row.get(13).unwrap_or_default(),
        count_spinner: row.get(14).unwrap_or_default(),
        submit_date: row.get(15).unwrap_or_default(),
        approved_date: row.get(16).unwrap_or_default(),
        last_update: row.get(17).unwrap_or_default(),
        artist: row.get(18).unwrap_or_default(),
        artist_unicode: row.get(19).unwrap_or_default(),
        title: row.get(20).unwrap_or_default(),
        title_unicode: row.get(21).unwrap_or_default(),
        creator: row.get(22).unwrap_or_default(),
        creator_id: row.get(23).unwrap_or_default(),
        bpm: row.get(24).unwrap_or_default(),
        source: row.get(25).unwrap_or_default(),
        tags: row.get(26).unwrap_or_default(),
        genre_id: row.get(27).unwrap_or_default(),
        language_id: row.get(28).unwrap_or_default(),
        favourite_count: row.get(29).unwrap_or_default(),
        rating: row.get(30).unwrap_or_default(),
        storyboard: row.get(31).unwrap_or_default(),
        video: row.get(32).unwrap_or_default(),
        download_unavailable: row.get(33).unwrap_or_default(),
        audio_unavailable: row.get(34).unwrap_or_default(),
        playcount: row.get(35).unwrap_or_default(),
        passcount: row.get(36).unwrap_or_default(),
        max_combo: row.get(37).unwrap_or_default(),
        diff_aim: row.get(38).unwrap_or_default(),
        diff_speed: row.get(39).unwrap_or_default(),
        difficultyrating: row.get(40).unwrap_or_default(),
        file_content: row.get(41).unwrap_or_default(),
    }
}

pub fn get_avatar(conn: &Connection, name: &str) -> SqlResult<Option<String>> {
    let mut stmt = conn.prepare("SELECT avatar_url FROM avatars WHERE player_name = ?1")?;
    let result = stmt.query_row(params![name], |row| row.get(0));
    match result {
        Ok(url) => Ok(url),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn set_avatar(conn: &Connection, name: &str, url: &str) -> SqlResult<()> {
    conn.execute("INSERT OR REPLACE INTO avatars (player_name, avatar_url) VALUES (?1, ?2)", params![name, url])?;
    Ok(())
}

pub fn ensure_avatar(conn: &Connection, name: &str) -> SqlResult<()> {
    conn.execute("INSERT OR IGNORE INTO avatars (player_name, avatar_url) VALUES (?1, NULL)", params![name])?;
    Ok(())
}

pub fn add_friend(conn: &Connection, player_name: &str, friend_id: i32, friend_name: &str) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO friends (player_name, friend_id, friend_name)
         VALUES (?1, ?2, ?3)
         ON CONFLICT(player_name, friend_id) DO UPDATE SET
            friend_name = CASE WHEN excluded.friend_name != '' THEN excluded.friend_name ELSE friends.friend_name END",
        params![player_name, friend_id, friend_name],
    )?;
    Ok(())
}

pub fn remove_friend(conn: &Connection, player_name: &str, friend_id: i32) -> SqlResult<()> {
    conn.execute("DELETE FROM friends WHERE player_name = ?1 AND friend_id = ?2", params![player_name, friend_id])?;
    Ok(())
}

pub fn get_friends(conn: &Connection, player_name: &str) -> SqlResult<Vec<(i32, String)>> {
    let mut stmt = conn.prepare("SELECT friend_id, friend_name FROM friends WHERE player_name = ?1 ORDER BY friend_id ASC")?;
    let rows = stmt.query_map(params![player_name], |row| Ok((row.get(0)?, row.get::<_, Option<String>>(1)?.unwrap_or_default())))?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn get_friend_ids(conn: &Connection, player_name: &str) -> SqlResult<Vec<i32>> {
    let mut stmt = conn.prepare("SELECT friend_id FROM friends WHERE player_name = ?1 ORDER BY friend_id ASC")?;
    let rows = stmt.query_map(params![player_name], |row| row.get(0))?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

#[derive(Debug, Clone)]
pub struct FriendRecord {
    pub friend_id: i32,
    pub friend_name: String,
    pub rank: i32,
    pub pp: i32,
    pub acc: f64,
    pub country: u8,
    pub ranked_score: i64,
    pub total_score: i64,
    pub playcount: i32,
}

pub fn save_friend_stats(conn: &Connection, player_name: &str, f: &FriendRecord) -> SqlResult<()> {
    conn.execute(
        "INSERT INTO friends (player_name, friend_id, friend_name, rank, pp, acc, country, ranked_score, total_score, playcount)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(player_name, friend_id) DO UPDATE SET
            friend_name = excluded.friend_name,
            rank = excluded.rank,
            pp = excluded.pp,
            acc = excluded.acc,
            country = excluded.country,
            ranked_score = excluded.ranked_score,
            total_score = excluded.total_score,
            playcount = excluded.playcount",
        params![player_name, f.friend_id, f.friend_name, f.rank, f.pp, f.acc, f.country as i32, f.ranked_score, f.total_score, f.playcount],
    )?;
    Ok(())
}

pub fn get_friend_records(conn: &Connection, player_name: &str) -> SqlResult<Vec<FriendRecord>> {
    let mut stmt = conn.prepare(
        "SELECT friend_id, friend_name, rank, pp, acc, country, ranked_score, total_score, playcount
         FROM friends
         WHERE player_name = ?1
         ORDER BY friend_id ASC",
    )?;
    let rows = stmt.query_map(params![player_name], |row| {
        Ok(FriendRecord {
            friend_id: row.get(0)?,
            friend_name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            rank: row.get::<_, Option<i32>>(2)?.unwrap_or(1),
            pp: row.get::<_, Option<i32>>(3)?.unwrap_or(0),
            acc: row.get::<_, Option<f64>>(4)?.unwrap_or(0.0),
            country: row.get::<_, Option<i32>>(5)?.unwrap_or(0) as u8,
            ranked_score: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            total_score: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            playcount: row.get::<_, Option<i32>>(8)?.unwrap_or(0),
        })
    })?;
    let mut list = Vec::new();
    for r in rows {
        list.push(r?);
    }
    Ok(list)
}

pub fn set_beatmap_status(conn: &Connection, beatmap_id: i64, approved: i32) -> SqlResult<usize> {
    let status_str = match approved {
        1 | 2 => "ranked",
        4 => "loved",
        _ => "unranked",
    };
    let _ =
        conn.execute("UPDATE scores SET bmap_status = ?1 WHERE md5 IN (SELECT file_md5 FROM beatmaps WHERE beatmap_id = ?2)", params![status_str, beatmap_id]);
    conn.execute("UPDATE beatmaps SET approved = ?1 WHERE beatmap_id = ?2", params![approved, beatmap_id])
}

pub fn set_beatmap_status_by_md5(conn: &Connection, md5: &str, approved: i32) -> SqlResult<usize> {
    conn.execute("UPDATE beatmaps SET approved = ?1 WHERE file_md5 = ?2", params![approved, md5])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_db_init_and_profile_flow() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        assert!(!profile_exists(&conn, "TestUser").unwrap());
        ensure_profile(&conn, "TestUser").unwrap();
        assert!(profile_exists(&conn, "TestUser").unwrap());

        assert_eq!(get_avatar(&conn, "TestUser").unwrap(), None);
        set_avatar(&conn, "TestUser", "https://example.com/pfp.png").unwrap();
        assert_eq!(get_avatar(&conn, "TestUser").unwrap(), Some("https://example.com/pfp.png".to_string()));

        let score = Score {
            mode: 0,
            md5: "abc123md5".to_string(),
            name: "TestUser".to_string(),
            n300: 300,
            n100: 10,
            n50: 0,
            ngeki: 50,
            nkatu: 5,
            nmiss: 0,
            score: 1000000,
            max_combo: 500,
            perfect: true,
            mods: 0,
            time: 12345678,
            acc: Some(99.5),
            pp: Some(250.0),
            replay_md5: Some("rep123".to_string()),
            scoreid: None,
            replay_frames: None,
            mods_str: Some("NM".to_string()),
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };

        insert_score(&conn, &score, "ranked").unwrap();
        let ranked_scores = get_ranked_scores(&conn, "TestUser").unwrap();
        assert_eq!(ranked_scores.len(), 1);
        assert_eq!(ranked_scores[0].score, 1000000);
        assert_eq!(ranked_scores[0].pp, Some(250.0));

        increment_playcount(&conn, "TestUser").unwrap();
        let playcount = get_playcount(&conn, "TestUser").unwrap();
        assert_eq!(playcount, 1);
    }

    #[test]
    fn test_get_scores_on_map_mode_filter() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        ensure_profile(&conn, "Player1").unwrap();

        let mut score = Score {
            mode: 0,
            md5: "map_hash_1".to_string(),
            name: "Player1".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 500000,
            max_combo: 300,
            perfect: true,
            mods: 0,
            time: 1000,
            acc: Some(100.0),
            pp: Some(150.0),
            replay_md5: Some("rep_mode0".to_string()),
            scoreid: None,
            replay_frames: None,
            mods_str: Some("NM".to_string()),
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };

        insert_score(&conn, &score, "ranked").unwrap();

        score.mode = 1;
        score.replay_md5 = Some("rep_mode1".to_string());
        score.score = 600000;
        insert_score(&conn, &score, "ranked").unwrap();

        score.mode = 3;
        score.replay_md5 = Some("rep_mode3".to_string());
        score.score = 900000;
        insert_score(&conn, &score, "ranked").unwrap();

        let std_scores = get_scores_on_map(&conn, "Player1", "map_hash_1", 0).unwrap();
        assert_eq!(std_scores.len(), 1);
        assert_eq!(std_scores[0].mode, 0);
        assert_eq!(std_scores[0].score, 500000);

        let taiko_scores = get_scores_on_map(&conn, "Player1", "map_hash_1", 1).unwrap();
        assert_eq!(taiko_scores.len(), 1);
        assert_eq!(taiko_scores[0].mode, 1);
        assert_eq!(taiko_scores[0].score, 600000);

        let ctb_scores = get_scores_on_map(&conn, "Player1", "map_hash_1", 2).unwrap();
        assert_eq!(ctb_scores.len(), 0);

        let mania_scores = get_scores_on_map(&conn, "Player1", "map_hash_1", 3).unwrap();
        assert_eq!(mania_scores.len(), 1);
        assert_eq!(mania_scores[0].mode, 3);
        assert_eq!(mania_scores[0].score, 900000);
    }

    #[test]
    fn test_friends_crud() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        assert_eq!(get_friends(&conn, "Alice").unwrap().len(), 0);
        assert_eq!(get_friend_ids(&conn, "Alice").unwrap().len(), 0);

        add_friend(&conn, "Alice", 100, "Bob").unwrap();
        add_friend(&conn, "Alice", 101, "Charlie").unwrap();

        let friends = get_friends(&conn, "Alice").unwrap();
        assert_eq!(friends.len(), 2);
        assert_eq!(friends[0], (100, "Bob".to_string()));
        assert_eq!(friends[1], (101, "Charlie".to_string()));

        let friend_ids = get_friend_ids(&conn, "Alice").unwrap();
        assert_eq!(friend_ids, vec![100, 101]);

        remove_friend(&conn, "Alice", 100).unwrap();
        let friend_ids_after = get_friend_ids(&conn, "Alice").unwrap();
        assert_eq!(friend_ids_after, vec![101]);

        // Calling add_friend with an empty name must not overwrite existing name
        add_friend(&conn, "Alice", 101, "").unwrap();
        let friends_preserved = get_friends(&conn, "Alice").unwrap();
        assert_eq!(friends_preserved[0].1, "Charlie");
    }

    #[test]
    fn test_beatmap_status_updates() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();

        let mut bmap = Beatmap::blank();
        bmap.file_md5 = "status_test_md5".to_string();
        bmap.beatmap_id = 9999;
        bmap.approved = 0;
        insert_beatmap(&conn, &bmap).unwrap();

        let loaded = get_beatmap_by_id(&conn, 9999).unwrap().unwrap();
        assert_eq!(loaded.approved, 0);

        let rows = set_beatmap_status(&conn, 9999, 1).unwrap();
        assert_eq!(rows, 1);
        let loaded_ranked = get_beatmap_by_id(&conn, 9999).unwrap().unwrap();
        assert_eq!(loaded_ranked.approved, 1);

        let rows_md5 = set_beatmap_status_by_md5(&conn, "status_test_md5", 4).unwrap();
        assert_eq!(rows_md5, 1);
        let loaded_loved = get_beatmap_by_md5(&conn, "status_test_md5").unwrap().unwrap();
        assert_eq!(loaded_loved.approved, 4);
    }

    #[test]
    fn test_get_max_combo_for_player() {
        let conn = Connection::open_in_memory().unwrap();
        init_db(&conn).unwrap();
        ensure_profile(&conn, "Alice").unwrap();

        // When no scores exist, returns 0
        assert_eq!(get_max_combo_for_player(&conn, "Alice", 0).unwrap(), 0);

        let make_score = |name: &str, mode: i32, md5: &str, max_combo: i32| Score {
            mode,
            md5: md5.to_string(),
            name: name.to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss: 0,
            score: 100000,
            max_combo,
            perfect: false,
            mods: 0,
            time: 0,
            acc: Some(100.0),
            pp: Some(100.0),
            replay_md5: None,
            scoreid: None,
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        };

        let sc1 = make_score("Alice", 0, "map1", 450);
        insert_score(&conn, &sc1, "ranked").unwrap();

        let sc2 = make_score("Alice", 0, "map2", 850);
        insert_score(&conn, &sc2, "ranked").unwrap();

        // Another mode
        let sc3 = make_score("Alice", 1, "map3", 1200);
        insert_score(&conn, &sc3, "ranked").unwrap();

        // Mode 0 max combo is 850
        assert_eq!(get_max_combo_for_player(&conn, "Alice", 0).unwrap(), 850);
        // Mode 1 max combo is 1200
        assert_eq!(get_max_combo_for_player(&conn, "Alice", 1).unwrap(), 1200);
        // Other player is 0
        assert_eq!(get_max_combo_for_player(&conn, "Bob", 0).unwrap(), 0);
    }
}
