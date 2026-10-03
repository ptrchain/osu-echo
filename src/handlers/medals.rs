use crate::db;
use crate::types::beatmap::Beatmap;
use crate::types::mods::Mods;
use crate::types::score::Score;
use rusqlite::Connection;
use std::collections::HashSet;

/// Evaluates a score submission (or existing score) against osu! medal criteria.
/// Returns a list of newly earned medal IDs (which are not in `existing_medals`).
pub fn evaluate_score_submission(
    score: &Score,
    bmap: &Beatmap,
    parsed_map: Option<&rosu_pp::Beatmap>,
    playcount: i32,
    existing_medals: &HashSet<i32>,
    passed: bool,
) -> Vec<i32> {
    let mut newly_earned = Vec::new();
    let mut check_and_award = |id: i32| {
        if !existing_medals.contains(&id) && !newly_earned.contains(&id) {
            newly_earned.push(id);
        }
    };

    // 1. Playcount Dedication Milestones (Any mode, passed or failed)
    // 5k (20), 15k (21), 25k (22), 50k (28)
    let playcount_milestones = [
        (5_000, 20),
        (15_000, 21),
        (25_000, 22),
        (50_000, 28),
    ];
    for &(threshold, medal_id) in &playcount_milestones {
        if playcount >= threshold {
            check_and_award(medal_id);
        }
    }

    // If failed, check Consolation Prize (ID 38)
    if !passed {
        let total_objects = bmap.count_normal + bmap.count_slider + bmap.count_spinner;
        let hit_objects = score.n300 + score.n100 + score.n50 + score.nmiss;
        if total_objects >= 50 && (hit_objects as f64 / total_objects as f64) >= 0.95 {
            check_and_award(38);
        }
        return newly_earned;
    }

    let score_mods = Mods::from_bits_truncate(score.mods);
    let is_unranked_mods = score_mods.intersects(Mods::RELAX | Mods::AUTOPILOT | Mods::AUTOPLAY | Mods::CINEMA);

    // Standard osu! rules require no unranked/cheat mods
    if is_unranked_mods {
        return newly_earned;
    }

    // 2. Combo Milestones (Standard osu! mode 0)
    // 500 (1), 750 (3), 1,000 (4), 2,000 (5)
    if score.mode == 0 {
        let combo_milestones = [
            (500, 1),
            (750, 3),
            (1000, 4),
            (2000, 5),
        ];
        for &(threshold, medal_id) in &combo_milestones {
            if score.max_combo >= threshold {
                check_and_award(medal_id);
            }
        }
    }

    // 3. Star Rating Passes & FCs (Standard osu! mode 0)
    if score.mode == 0 {
        let stars = if let Some(parsed) = parsed_map {
            rosu_pp::Difficulty::new().mods(score.mods).calculate(parsed).stars()
        } else {
            bmap.difficultyrating
        };

        // 1★–10★ Passes: 55..=62, 242 (9★), 244 (10★)
        let pass_medals = [
            (1.0, 55),
            (2.0, 56),
            (3.0, 57),
            (4.0, 58),
            (5.0, 59),
            (6.0, 60),
            (7.0, 61),
            (8.0, 62),
            (9.0, 242),
            (10.0, 244),
        ];
        for &(req_stars, medal_id) in &pass_medals {
            if stars >= req_stars {
                check_and_award(medal_id);
            }
        }

        // 1★–10★ FCs: 63..=70, 243 (9★), 245 (10★)
        // FC requires no misses and no sliderbreaks (score.perfect or combo near map max)
        let is_fc = score.nmiss == 0 && (score.perfect || bmap.max_combo == 0 || score.max_combo >= bmap.max_combo - 1);
        if is_fc {
            let fc_medals = [
                (1.0, 63),
                (2.0, 64),
                (3.0, 65),
                (4.0, 66),
                (5.0, 67),
                (6.0, 68),
                (7.0, 69),
                (8.0, 70),
                (9.0, 243),
                (10.0, 245),
            ];
            for &(req_stars, medal_id) in &fc_medals {
                if stars >= req_stars {
                    check_and_award(medal_id);
                }
            }
        }
    }

    // 4. Mod Introductions (Passed, standard mode)
    if score.mode == 0 {
        if score_mods.contains(Mods::SUDDENDEATH) && !score_mods.contains(Mods::PERFECT) {
            check_and_award(119); // Finality
        }
        if score_mods.contains(Mods::PERFECT) {
            check_and_award(120); // Perfectionist
        }
        if score_mods.contains(Mods::HARDROCK) {
            check_and_award(121); // Rock Around The Clock
        }
        if score_mods.contains(Mods::DOUBLETIME) && !score_mods.contains(Mods::NIGHTCORE) {
            check_and_award(122); // Time And A Half
        }
        if score_mods.contains(Mods::NIGHTCORE) {
            check_and_award(123); // Sweet Rave Party
        }
        if score_mods.contains(Mods::HIDDEN) {
            check_and_award(124); // Blindsight
        }
        if score_mods.contains(Mods::FLASHLIGHT) {
            check_and_award(125); // Are You Afraid Of The Dark?
        }
        if score_mods.contains(Mods::EASY) {
            check_and_award(126); // Dial It Right Back
        }
        if score_mods.contains(Mods::NOFAIL) {
            check_and_award(127); // Risk Averse
        }
        if score_mods.contains(Mods::HALFTIME) {
            check_and_award(128); // Slowboat
        }
        if score_mods.contains(Mods::SPUNOUT) {
            check_and_award(131); // Burned Out
        }
    }

    // 5. Popular Hush-Hush / Secrets (Passed)
    // Jackpot: score ends with 77,777 or total score == 777,777 (ID 41)
    if score.score % 100_000 == 77_777 || score.score == 777_777 {
        check_and_award(41);
    }
    // Nonstop: Beatmap drain / hit length >= 600s (10 min) and FC / no miss (ID 44)
    if bmap.hit_length >= 600 && score.nmiss == 0 {
        check_and_award(44);
    }

    newly_earned
}

/// Scans existing player scores and profile metrics in SQLite to award all qualifying medals historically.
pub fn retroactive_eval_profile(conn: &Connection, player_name: &str) -> rusqlite::Result<Vec<i32>> {
    let existing_medals: HashSet<i32> = db::get_user_medals(conn, player_name)?
        .into_iter()
        .map(|m| m.medal_id)
        .collect();

    let playcount = db::get_playcount(conn, player_name).unwrap_or(0);
    let mut newly_earned = Vec::new();
    let mut check_and_award = |id: i32| {
        if !existing_medals.contains(&id) && !newly_earned.contains(&id) {
            newly_earned.push(id);
        }
    };

    // 1. Playcount dedication
    let playcount_milestones = [
        (5_000, 20),
        (15_000, 21),
        (25_000, 22),
        (50_000, 28),
    ];
    for &(threshold, medal_id) in &playcount_milestones {
        if playcount >= threshold {
            check_and_award(medal_id);
        }
    }

    // 2. Scan all saved scores
    let all_scores = db::get_all_scores(conn, player_name)?;
    for score in all_scores {
        let bmap = db::get_beatmap_by_md5(conn, &score.md5)?
            .unwrap_or_else(Beatmap::blank);

        let parsed_map = bmap.file_content.as_deref().and_then(|c| rosu_pp::Beatmap::from_bytes(c.as_bytes()).ok());

        let awarded = evaluate_score_submission(
            &score,
            &bmap,
            parsed_map.as_ref(),
            playcount,
            &existing_medals,
            true, // Saved scores are passes
        );

        for id in awarded {
            check_and_award(id);
        }
    }

    if !newly_earned.is_empty() {
        let now = chrono::Utc::now().timestamp();
        let batch: Vec<(i32, i64)> = newly_earned.iter().map(|&id| (id, now)).collect();
        db::add_user_medals_batch(conn, player_name, &batch)?;
    }

    Ok(newly_earned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_score(max_combo: i32, nmiss: i32, perfect: bool, mods: u32, score_val: i64) -> Score {
        Score {
            mode: 0,
            md5: "test_md5".to_string(),
            name: "Alice".to_string(),
            n300: 300,
            n100: 0,
            n50: 0,
            ngeki: 0,
            nkatu: 0,
            nmiss,
            score: score_val,
            max_combo,
            perfect,
            mods,
            time: 1700000000,
            acc: Some(100.0),
            pp: Some(100.0),
            replay_md5: None,
            scoreid: None,
            replay_frames: None,
            mods_str: None,
            additional_mods: None,
            submission_checksum: None,
            submission_identity: None,
        }
    }

    #[test]
    fn test_medal_combo_evaluation() {
        let bmap = Beatmap::blank();
        let existing = HashSet::new();

        // 499 combo -> no combo medal
        let s499 = make_test_score(499, 0, true, 0, 500000);
        let medals = evaluate_score_submission(&s499, &bmap, None, 10, &existing, true);
        assert!(!medals.contains(&1));

        // 500 combo -> awards ID 1
        let s500 = make_test_score(500, 0, true, 0, 500000);
        let medals = evaluate_score_submission(&s500, &bmap, None, 10, &existing, true);
        assert!(medals.contains(&1));
        assert!(!medals.contains(&3));

        // 1000 combo -> awards ID 1, 3, 4
        let s1000 = make_test_score(1000, 0, true, 0, 1000000);
        let medals = evaluate_score_submission(&s1000, &bmap, None, 10, &existing, true);
        assert!(medals.contains(&1));
        assert!(medals.contains(&3));
        assert!(medals.contains(&4));
        assert!(!medals.contains(&5));

        // When 1 and 3 already exist:
        let mut existing_with_some = HashSet::new();
        existing_with_some.insert(1);
        existing_with_some.insert(3);
        let medals = evaluate_score_submission(&s1000, &bmap, None, 10, &existing_with_some, true);
        assert_eq!(medals, vec![4]);
    }

    #[test]
    fn test_medal_star_pass_and_fc_evaluation() {
        let mut bmap = Beatmap::blank();
        bmap.difficultyrating = 5.25;
        bmap.max_combo = 400;

        let existing = HashSet::new();

        // Score with miss -> passes 1★–5★, but NO FC medals
        let pass_score = make_test_score(350, 1, false, 0, 600000);
        let medals = evaluate_score_submission(&pass_score, &bmap, None, 10, &existing, true);
        assert!(medals.contains(&55)); // 1★ pass
        assert!(medals.contains(&59)); // 5★ pass
        assert!(!medals.contains(&60)); // 6★ pass
        assert!(!medals.contains(&63)); // 1★ FC should NOT be awarded

        // Score FC -> awards passes 1★–5★ AND FCs 1★–5★
        let fc_score = make_test_score(400, 0, true, 0, 800000);
        let medals = evaluate_score_submission(&fc_score, &bmap, None, 10, &existing, true);
        assert!(medals.contains(&55));
        assert!(medals.contains(&59));
        assert!(medals.contains(&63)); // 1★ FC
        assert!(medals.contains(&67)); // 5★ FC
        assert!(!medals.contains(&68)); // 6★ FC
    }

    #[test]
    fn test_medal_mod_introductions() {
        let bmap = Beatmap::blank();
        let existing = HashSet::new();

        // Hidden + HardRock
        let hdhr = (Mods::HIDDEN | Mods::HARDROCK).bits();
        let s = make_test_score(100, 0, true, hdhr, 100000);
        let medals = evaluate_score_submission(&s, &bmap, None, 10, &existing, true);
        assert!(medals.contains(&121)); // Rock Around The Clock (HR)
        assert!(medals.contains(&124)); // Blindsight (HD)

        // DoubleTime
        let dt = Mods::DOUBLETIME.bits();
        let s_dt = make_test_score(100, 0, true, dt, 100000);
        let medals_dt = evaluate_score_submission(&s_dt, &bmap, None, 10, &existing, true);
        assert!(medals_dt.contains(&122)); // Time And A Half (DT)

        // Relax / unranked mods should not award mod intro medals
        let rx = (Mods::DOUBLETIME | Mods::RELAX).bits();
        let s_rx = make_test_score(100, 0, true, rx, 100000);
        let medals_rx = evaluate_score_submission(&s_rx, &bmap, None, 10, &existing, true);
        assert!(medals_rx.is_empty());
    }

    #[test]
    fn test_medal_secrets_jackpot_and_consolation() {
        let mut bmap = Beatmap::blank();
        bmap.count_normal = 80;
        bmap.count_slider = 20;

        let existing = HashSet::new();

        // Jackpot score ending in 77,777
        let jackpot_score = make_test_score(100, 0, true, 0, 1_477_777);
        let medals = evaluate_score_submission(&jackpot_score, &bmap, None, 10, &existing, true);
        assert!(medals.contains(&41)); // Jackpot

        // Consolation Prize: fail at >= 95% of map
        let mut fail_score = make_test_score(100, 1, false, 0, 300000);
        fail_score.n300 = 96; // 96 out of 100 objects hit
        let medals_fail = evaluate_score_submission(&fail_score, &bmap, None, 10, &existing, false);
        assert!(medals_fail.contains(&38)); // Consolation Prize
    }

    #[test]
    fn test_retroactive_eval_profile() {
        let conn = Connection::open_in_memory().unwrap();
        db::init_db(&conn).unwrap();
        db::ensure_profile(&conn, "RetroTester").unwrap();

        // Add playcount
        for _ in 0..5005 {
            db::increment_playcount(&conn, "RetroTester").unwrap();
        }

        // Add a 500 combo score on a 4.5★ map
        let mut bmap = Beatmap::blank();
        bmap.file_md5 = "retro_map_md5".to_string();
        bmap.difficultyrating = 4.5;
        bmap.max_combo = 600;
        db::insert_beatmap(&conn, &bmap).unwrap();

        let mut sc = make_test_score(550, 0, true, Mods::HARDROCK.bits(), 750000);
        sc.name = "RetroTester".to_string();
        sc.md5 = "retro_map_md5".to_string();
        db::insert_score(&conn, &sc, "ranked").unwrap();

        // Run retroactive evaluation
        let new_medals = retroactive_eval_profile(&conn, "RetroTester").unwrap();

        // Should include playcount 5k (20), 500 combo (1), passes 1★–4★ (55..=58), FCs 1★–4★ (63..=66), HardRock (121)
        assert!(new_medals.contains(&20));
        assert!(new_medals.contains(&1));
        assert!(new_medals.contains(&55));
        assert!(new_medals.contains(&58));
        assert!(new_medals.contains(&63));
        assert!(new_medals.contains(&66));
        assert!(new_medals.contains(&121));

        // Re-running retroactive evaluation yields 0 new medals
        let rerun = retroactive_eval_profile(&conn, "RetroTester").unwrap();
        assert!(rerun.is_empty());

        // Verify count in database
        let count = db::get_user_medal_count(&conn, "RetroTester").unwrap();
        assert_eq!(count, new_medals.len());
    }
}
