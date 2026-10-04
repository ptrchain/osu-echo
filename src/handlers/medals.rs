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
    mode_total_hits: i64,
    existing_medals: &HashSet<i32>,
    passed: bool,
) -> Vec<i32> {
    let mut newly_earned = Vec::new();
    let mut check_and_award = |id: i32| {
        if !existing_medals.contains(&id) && !newly_earned.contains(&id) {
            newly_earned.push(id);
        }
    };

    // 1. Playcount & Hit Dedication Milestones (Passed or failed)
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

    // Mode-specific cumulative hit milestones
    let hit_milestones: &[(i64, i32)] = match score.mode {
        1 => &[
            (30_000, 31),
            (300_000, 32),
            (3_000_000, 33),
            (30_000_000, 291),
        ],
        2 => &[
            (20_000, 13),
            (200_000, 23),
            (2_000_000, 24),
            (20_000_000, 292),
        ],
        3 => &[
            (40_000, 46),
            (400_000, 47),
            (4_000_000, 48),
            (40_000_000, 293),
        ],
        _ => &[],
    };
    for &(threshold, medal_id) in hit_milestones {
        if mode_total_hits >= threshold {
            check_and_award(medal_id);
        }
    }

    // If failed, check Consolation Prize (ID 38), True Torment (ID 173), Just One Second (ID 135)
    if !passed {
        let total_objects = bmap.count_normal + bmap.count_slider + bmap.count_spinner;
        let hit_objects = score.n300 + score.n100 + score.n50 + score.nmiss;
        if total_objects >= 50 && (hit_objects as f64 / total_objects as f64) >= 0.95 {
            check_and_award(38);
        }
        if total_objects >= 50 && (hit_objects as f64 / total_objects as f64) >= 0.99 {
            check_and_award(173);
        }
        if total_objects >= 20 && hit_objects > 0 && hit_objects <= 3 {
            check_and_award(135);
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

    let game_mode = match score.mode {
        0 => rosu_pp::model::mode::GameMode::Osu,
        1 => rosu_pp::model::mode::GameMode::Taiko,
        2 => rosu_pp::model::mode::GameMode::Catch,
        3 => rosu_pp::model::mode::GameMode::Mania,
        _ => rosu_pp::model::mode::GameMode::Osu,
    };
    let stars = if let Some(parsed) = parsed_map {
        rosu_pp::Performance::new(parsed)
            .mode_or_ignore(game_mode)
            .mods(score.mods)
            .calculate()
            .difficulty_attributes()
            .stars()
    } else {
        bmap.difficultyrating
    };

    // 3. Star Rating Passes & FCs (All 4 Game Modes: osu!standard, Taiko, Catch, Mania)
    let pass_table: &[(f64, i32)] = match score.mode {
        0 => &[
            (1.0, 55), (2.0, 56), (3.0, 57), (4.0, 58), (5.0, 59),
            (6.0, 60), (7.0, 61), (8.0, 62), (9.0, 242), (10.0, 244),
        ],
        1 => &[
            (1.0, 71), (2.0, 72), (3.0, 73), (4.0, 74), (5.0, 75),
            (6.0, 76), (7.0, 77), (8.0, 78),
        ],
        2 => &[
            (1.0, 79), (2.0, 80), (3.0, 81), (4.0, 82), (5.0, 83),
            (6.0, 84), (7.0, 85), (8.0, 86),
        ],
        3 => &[
            (1.0, 87), (2.0, 88), (3.0, 89), (4.0, 90), (5.0, 91),
            (6.0, 92), (7.0, 93), (8.0, 94),
        ],
        _ => &[],
    };
    for &(req_stars, medal_id) in pass_table {
        if stars >= req_stars {
            check_and_award(medal_id);
        }
    }

    // FC detection
    let is_fc = score.nmiss == 0 && match score.mode {
        1 | 3 => true, // Taiko and Mania don't have sliderbreaks; 0 miss is FC
        _ => score.perfect || bmap.max_combo == 0 || score.max_combo >= bmap.max_combo - 1,
    };

    if is_fc {
        let fc_table: &[(f64, i32)] = match score.mode {
            0 => &[
                (1.0, 63), (2.0, 64), (3.0, 65), (4.0, 66), (5.0, 67),
                (6.0, 68), (7.0, 69), (8.0, 70), (9.0, 243), (10.0, 245),
            ],
            1 => &[
                (1.0, 95), (2.0, 96), (3.0, 97), (4.0, 98), (5.0, 99),
                (6.0, 100), (7.0, 101), (8.0, 102),
            ],
            2 => &[
                (1.0, 103), (2.0, 104), (3.0, 105), (4.0, 106), (5.0, 107),
                (6.0, 108), (7.0, 109), (8.0, 110),
            ],
            3 => &[
                (1.0, 111), (2.0, 112), (3.0, 113), (4.0, 114), (5.0, 115),
                (6.0, 116), (7.0, 117), (8.0, 118),
            ],
            _ => &[],
        };
        for &(req_stars, medal_id) in fc_table {
            if stars >= req_stars {
                check_and_award(medal_id);
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
    // Stumbler: Passed with D rank (accuracy < 70%) (ID 40)
    let acc = score.acc.unwrap_or(100.0);
    if acc > 0.0 && acc < 70.0 {
        check_and_award(40);
    }
    // Quick Draw: Passed a short beatmap (drain / hit length between 5s and 30s) (ID 42)
    if bmap.hit_length >= 5 && bmap.hit_length <= 30 {
        check_and_award(42);
    }
    // Afterimage: Passed a map with Hidden + HardRock (ID 136)
    if score_mods.contains(Mods::HIDDEN | Mods::HARDROCK) {
        check_and_award(136);
    }
    // When You See It: Score ends with 727, combo is 727, score == 727, or PP approx 727 (ID 287)
    if score.score % 1_000 == 727 || score.max_combo == 727 || score.score == 727 || (score.pp.unwrap_or(0.0) - 727.0).abs() < 0.5 {
        check_and_award(287);
    }
    // Permadeath: SuddenDeath with 0 misses on >= 4.0 stars (ID 220)
    if score_mods.contains(Mods::SUDDENDEATH) && score.nmiss == 0 && stars >= 4.0 {
        check_and_award(220);
    }
    // 50/50: 300s equal 100s with at least 30 of each (ID 168)
    if score.n300 == score.n100 && score.n100 >= 30 {
        check_and_award(168);
    }
    // Natural 20: Max combo is exactly 20 (ID 222)
    if score.max_combo == 20 {
        check_and_award(222);
    }
    // Too Fast, Too Furious: Passed with DT/NC on >= 200 BPM or >= 5.0 stars (ID 175)
    if score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && (bmap.bpm >= 200.0 || stars >= 5.0) {
        check_and_award(175);
    }
    // Eclipse: Passed with Hidden + Flashlight (ID 139)
    if score_mods.contains(Mods::HIDDEN | Mods::FLASHLIGHT) {
        check_and_award(139);
    }
    // Senseless: Passed "Silence" with Hidden (ID 197)
    let map_title_lower = bmap.title.to_lowercase();
    let map_artist_lower = bmap.artist.to_lowercase();
    if map_title_lower.contains("silence") && score_mods.contains(Mods::HIDDEN) {
        check_and_award(197);
    }
    // Reckless Abandon: FC on >= 3.0 stars with SuddenDeath + HardRock (ID 140)
    if is_fc && stars >= 3.0 && score_mods.contains(Mods::SUDDENDEATH | Mods::HARDROCK) {
        check_and_award(140);
    }
    // Lights Out: Passed with Nightcore + Flashlight (ID 144)
    if score_mods.contains(Mods::NIGHTCORE | Mods::FLASHLIGHT) {
        check_and_award(144);
    }
    // Tunnel Vision: Passed with Flashlight with combo < 200 on map with max combo >= 200 (ID 141)
    if score_mods.contains(Mods::FLASHLIGHT) && score.max_combo > 0 && score.max_combo < 200 && bmap.max_combo >= 200 {
        check_and_award(141);
    }
    // Behold No Deception: FC on >= 4.0 stars with Easy (ID 142)
    if is_fc && stars >= 4.0 && score_mods.contains(Mods::EASY) {
        check_and_award(142);
    }
    // Camera Shy: FC with Hidden + NoFail (ID 147)
    if is_fc && score_mods.contains(Mods::HIDDEN | Mods::NOFAIL) {
        check_and_award(147);
    }
    // The Sum Of All Fears: 1 miss on the final note of a map with >= 50 objects (ID 148)
    if score.nmiss == 1 && bmap.max_combo >= 50 && score.max_combo >= bmap.max_combo - 1 {
        check_and_award(148);
    }
    // Dekasight: Passed with Easy + Flashlight on >= 3.0 stars (ID 149)
    if score_mods.contains(Mods::EASY | Mods::FLASHLIGHT) && stars >= 3.0 {
        check_and_award(149);
    }
    // Slow And Steady: Passed with HalfTime on >= 3.0 stars (ID 151)
    if score_mods.contains(Mods::HALFTIME) && stars >= 3.0 {
        check_and_award(151);
    }
    // Prepared: FC with NoFail (ID 138)
    if is_fc && score_mods.contains(Mods::NOFAIL) {
        check_and_award(138);
    }
    // No Time To Spare: FC with DT/NC on a beatmap with hit length <= 30s (ID 152)
    if is_fc && score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && bmap.hit_length > 0 && bmap.hit_length <= 30 {
        check_and_award(152);
    }
    // Meticulous: SS (100% accuracy) on >= 3.0 stars with Easy + Perfect (ID 157)
    if is_fc && score.n100 == 0 && score.n50 == 0 && stars >= 3.0 && score_mods.contains(Mods::EASY | Mods::PERFECT) {
        check_and_award(157);
    }
    // Equilibrium: Equal 300s, 100s, and 50s judgments (minimum 15 of each) (ID 159)
    if score.n300 == score.n100 && score.n100 == score.n50 && score.n300 >= 15 {
        check_and_award(159);
    }
    // Impeccable: Passed on >= 4.0 stars with DT/NC + Perfect (ID 160)
    if score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && score_mods.contains(Mods::PERFECT) && stars >= 4.0 {
        check_and_award(160);
    }
    // To The Core: Passed a map with "Nightcore" in title or artist with DT/NC (ID 137)
    if (map_title_lower.contains("nightcore") || map_artist_lower.contains("nightcore")) && score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) {
        check_and_award(137);
    }
    // Hour Before The Dawn: FC any difficulty of "EOS" (ID 150)
    if is_fc && map_title_lower.contains("eos") {
        check_and_award(150);
    }
    // Sognare: Passed "Evanescent" with HalfTime + Hidden (ID 153)
    if map_title_lower.contains("evanescent") && score_mods.contains(Mods::HALFTIME | Mods::HIDDEN) {
        check_and_award(153);
    }
    // Realtor Extraordinaire: FC "House With Legs" with HD + HR + DT/NC (ID 154)
    if is_fc && map_title_lower.contains("house with legs") && score_mods.contains(Mods::HIDDEN | Mods::HARDROCK) && score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) {
        check_and_award(154);
    }
    // Eureka!: FC on a >= 4.0 star map by "The Flashbulb" (ID 218)
    if is_fc && stars >= 4.0 && map_artist_lower.contains("the flashbulb") {
        check_and_award(218);
    }
    // Regicide: FC "Chandelier" on >= 4.0 stars (ID 219)
    if is_fc && stars >= 4.0 && map_title_lower.contains("chandelier") {
        check_and_award(219);
    }
    // The Future Is Now: Passed "Exit This Earth's Atomosphere" (ID 221)
    if map_title_lower.contains("exit this earth's atomosphere") {
        check_and_award(221);
    }
    // Valediction: Passed "Alexithymia" or "Lupinus" with >= 90% acc (ID 225)
    if (map_title_lower.contains("alexithymia") || map_title_lower.contains("lupinus")) && acc >= 90.0 {
        check_and_award(225);
    }
    // In Memoriam: SS FC on >= 4.0 stars with EZ + FL + HD + HT (ID 277)
    if score.mode == 0 && is_fc && score.n100 == 0 && score.n50 == 0 && stars >= 4.0 && score_mods.contains(Mods::EASY | Mods::FLASHLIGHT | Mods::HIDDEN | Mods::HALFTIME) {
        check_and_award(277);
    }
    // Sanguine: Passed "Scarlet Rose" with EZ and >= 92% acc (ID 278)
    if map_title_lower.contains("scarlet rose") && score_mods.contains(Mods::EASY) && acc >= 92.0 {
        check_and_award(278);
    }
    // Final Boss: Passed "RPG" (ID 280)
    if map_title_lower.contains("rpg") && (map_artist_lower.contains("yooh") || bmap.version.to_lowercase().contains("divinity")) {
        check_and_award(280);
    }
    // Beast Mode: Passed "Feral" with >= 98% acc (ID 281)
    if map_title_lower.contains("feral") && acc >= 98.0 {
        check_and_award(281);
    }
    // Unstoppable: Passed with DT/NC + HR on high-spec map (BPM >= 260 or stars >= 7.0) (ID 145)
    if score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && score_mods.contains(Mods::HARDROCK) && (bmap.bpm >= 260.0 || stars >= 7.0) {
        check_and_award(145);
    }
    // Is This Real Life?: FC with DT/NC + HR on high-spec map (BPM >= 260 or stars >= 7.0) (ID 146)
    if is_fc && score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && score_mods.contains(Mods::HARDROCK) && (bmap.bpm >= 260.0 || stars >= 7.0) {
        check_and_award(146);
    }
    // Time Dilation: HalfTime on a map with hit length >= 300s (ID 134)
    if score_mods.contains(Mods::HALFTIME) && bmap.hit_length >= 300 {
        check_and_award(134);
    }
    // Feelin' It: Flashlight with combo >= 500 (ID 176)
    if score_mods.contains(Mods::FLASHLIGHT) && score.max_combo >= 500 {
        check_and_award(176);
    }
    // Overconfident: HardRock on a map with >= 5.0 stars (ID 177)
    if score_mods.contains(Mods::HARDROCK) && stars >= 5.0 {
        check_and_award(177);
    }
    // Challenge Accepted: HardRock on a map with >= 3.5 stars (ID 39)
    if score_mods.contains(Mods::HARDROCK) && stars >= 3.5 {
        check_and_award(39);
    }
    // Dead Center: FC on a map with >= 3.0 stars and equal number of circles and sliders (ID 276)
    if is_fc && stars >= 3.0 && bmap.count_normal > 0 && bmap.count_normal == bmap.count_slider {
        check_and_award(276);
    }
    // Don't let the bunny distract you! (ID 6): Pass Daydream cafe or Gochuumon
    if map_title_lower.contains("daydream caf") || map_title_lower.contains("gochuumon") || map_artist_lower.contains("petit rabbit") {
        check_and_award(6);
    }
    // Up For The Challenge (ID 143): Pass with HD + HR + DT/NC
    if score_mods.contains(Mods::HIDDEN | Mods::HARDROCK) && score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) {
        check_and_award(143);
    }
    // Thrill of the Chase (ID 170): FC with Flashlight on >= 4.0 stars
    if is_fc && score_mods.contains(Mods::FLASHLIGHT) && stars >= 4.0 {
        check_and_award(170);
    }
    // You Can't Hide (ID 172): FC with Hidden on >= 5.0 stars
    if is_fc && score_mods.contains(Mods::HIDDEN) && stars >= 5.0 {
        check_and_award(172);
    }
    // Spooked (ID 178): Pass a spooky or Halloween themed map
    if map_title_lower.contains("spook") || map_title_lower.contains("halloween") || map_title_lower.contains("ghost") || map_title_lower.contains("witch") {
        check_and_award(178);
    }
    // Any% (ID 194): Pass a beatmap with hit length <= 20s with DT/NC
    if score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && bmap.hit_length > 0 && bmap.hit_length <= 20 {
        check_and_award(194);
    }
    // Not Bluffing (ID 217): Pass with Hidden + Flashlight on >= 4.0 stars
    if score_mods.contains(Mods::HIDDEN | Mods::FLASHLIGHT) && stars >= 4.0 {
        check_and_award(217);
    }
    // Ten To One (ID 268): Max combo is exactly 10
    if score.max_combo == 10 {
        check_and_award(268);
    }
    // Deliberation (ID 285): Pass with HalfTime on >= 4.0 stars
    if score_mods.contains(Mods::HALFTIME) && stars >= 4.0 {
        check_and_award(285);
    }
    // Lightless (ID 286): Pass with Hidden + HardRock + Flashlight
    if score_mods.contains(Mods::HIDDEN | Mods::HARDROCK | Mods::FLASHLIGHT) {
        check_and_award(286);
    }
    // The Strongest Ice Fairy (ID 347): Pass Chirumiru or Beloved Tomboyish Girl / Cirno
    if map_title_lower.contains("chirumiru") || map_title_lower.contains("tomboyish") || map_title_lower.contains("cirno") {
        check_and_award(347);
    }
    // Up To Eleven (ID 352): Pass with DT/NC + HR on >= 8.0 stars
    if score_mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && score_mods.contains(Mods::HARDROCK) && stars >= 8.0 {
        check_and_award(352);
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

    // Mode-specific cumulative hit milestones
    let taiko_hits = db::get_mode_total_hits(conn, player_name, 1).unwrap_or(0);
    let catch_hits = db::get_mode_total_hits(conn, player_name, 2).unwrap_or(0);
    let mania_hits = db::get_mode_total_hits(conn, player_name, 3).unwrap_or(0);

    let mode_hit_milestones = [
        (taiko_hits, &[(30_000, 31), (300_000, 32), (3_000_000, 33), (30_000_000, 291)][..]),
        (catch_hits, &[(20_000, 13), (200_000, 23), (2_000_000, 24), (20_000_000, 292)][..]),
        (mania_hits, &[(40_000, 46), (400_000, 47), (4_000_000, 48), (40_000_000, 293)][..]),
    ];
    for &(total, list) in &mode_hit_milestones {
        for &(thresh, id) in list {
            if total >= thresh {
                check_and_award(id);
            }
        }
    }

    // 2. Scan all saved scores
    let all_scores = db::get_all_scores(conn, player_name)?;
    for score in &all_scores {
        let bmap = db::get_beatmap_by_md5(conn, &score.md5)?
            .unwrap_or_else(Beatmap::blank);

        let parsed_map = bmap.file_content.as_deref().and_then(|c| rosu_pp::Beatmap::from_bytes(c.as_bytes()).ok());
        let mode_hits = match score.mode {
            1 => taiko_hits,
            2 => catch_hits,
            3 => mania_hits,
            _ => 0,
        };

        let awarded = evaluate_score_submission(
            score,
            &bmap,
            parsed_map.as_ref(),
            playcount,
            mode_hits,
            &existing_medals,
            true, // Saved scores are passes
        );

        for id in awarded {
            check_and_award(id);
        }
    }

    // S-Ranker (ID 15): At least 3 S or SS scores on distinct beatmaps
    let s_rank_maps: HashSet<&str> = all_scores
        .iter()
        .filter(|s| s.nmiss == 0 && s.acc.unwrap_or(0.0) >= 95.0)
        .map(|s| s.md5.as_str())
        .collect();
    if s_rank_maps.len() >= 3 {
        check_and_award(15);
    }

    // Jack of All Trades (ID 45): Played across all 4 modes
    let mut modes_played = HashSet::new();
    for s in &all_scores {
        modes_played.insert(s.mode);
    }
    if modes_played.contains(&0) && modes_played.contains(&1) && modes_played.contains(&2) && modes_played.contains(&3) {
        check_and_award(45);
    }

    // 3. Dedication & Beatmap Pack Progress
    let mut map_play_counts: std::collections::HashMap<&str, i32> = std::collections::HashMap::new();
    let mut artist_counts: std::collections::HashMap<String, i32> = std::collections::HashMap::new();
    let mut artist_distinct_maps: std::collections::HashMap<String, HashSet<&str>> = std::collections::HashMap::new();
    let mut genre_distinct_maps: std::collections::HashMap<&str, HashSet<&str>> = std::collections::HashMap::new();

    for score in &all_scores {
        *map_play_counts.entry(score.md5.as_str()).or_insert(0) += 1;

        if let Ok(Some(bm)) = db::get_beatmap_by_md5(conn, &score.md5) {
            let art_lower = bm.artist.to_lowercase();
            *artist_counts.entry(art_lower.clone()).or_insert(0) += 1;
            artist_distinct_maps.entry(art_lower).or_default().insert(score.md5.as_str());

            let tags_lower = format!("{} {} {}", bm.tags.to_lowercase(), bm.source.to_lowercase(), bm.title.to_lowercase());

            // Video Game
            if bm.genre_id == 2 || tags_lower.contains("game") || tags_lower.contains("nintendo") || tags_lower.contains("pokemon") {
                genre_distinct_maps.entry("video_game").or_default().insert(score.md5.as_str());
            }
            // Anime
            if bm.genre_id == 3 || tags_lower.contains("anime") || tags_lower.contains("ost") || tags_lower.contains("opening") || tags_lower.contains("ending") {
                genre_distinct_maps.entry("anime").or_default().insert(score.md5.as_str());
            }
            // Internet!
            if tags_lower.contains("meme") || tags_lower.contains("internet") || tags_lower.contains("remix") || tags_lower.contains("nico") {
                genre_distinct_maps.entry("internet").or_default().insert(score.md5.as_str());
            }
            // Rhythm Game
            if tags_lower.contains("rhythm") || tags_lower.contains("bms") || tags_lower.contains("stepmania") || tags_lower.contains("chunithm") || tags_lower.contains("sound voltex") || tags_lower.contains("sdvx") {
                genre_distinct_maps.entry("rhythm").or_default().insert(score.md5.as_str());
            }
            // Touhou
            if tags_lower.contains("touhou") || tags_lower.contains("zun") || tags_lower.contains("東方") {
                genre_distinct_maps.entry("touhou").or_default().insert(score.md5.as_str());
            }
            // Drum & Bass
            if tags_lower.contains("drum & bass") || tags_lower.contains("dnb") {
                genre_distinct_maps.entry("dnb").or_default().insert(score.md5.as_str());
            }
            // Chill
            if tags_lower.contains("chill") || tags_lower.contains("lo-fi") || tags_lower.contains("lofi") {
                genre_distinct_maps.entry("chill").or_default().insert(score.md5.as_str());
            }
        }
    }

    // Persistence Is Key (ID 270: >= 25 plays on same map) & Obsessed (ID 43: >= 50 plays)
    for &count in map_play_counts.values() {
        if count >= 25 {
            check_and_award(270);
        }
        if count >= 50 {
            check_and_award(43);
        }
    }

    // Superfan (ID 306: >= 15 scores for one artist)
    for &count in artist_counts.values() {
        if count >= 15 {
            check_and_award(306);
        }
    }

    // Featured Artist Packs (Requires 3+ distinct completed maps)
    let fa_packs: &[(&str, i32)] = &[
        ("camellia", 246),
        ("かめりあ", 246),
        ("celldweller", 248),
        ("cranky", 189),
        ("high tea music", 191),
        ("the flashbulb", 238),
        ("undead corporation", 239),
        ("s3rl", 259),
        ("leaf", 254),
        ("panda eyes", 255),
        ("teminite", 261),
        ("usao", 308),
        ("ginkiha", 283),
        ("hyper potions", 252),
        ("lukhash", 235),
        ("creo", 232),
        ("cysmix", 233),
        ("mili", 361),
        ("chroma", 360),
        ("motoloid", 179),
        ("culprate", 206),
        ("hyun", 208),
        ("imperial circus dead decadence", 213),
        ("tieff", 214),
        ("afterparty", 229),
        ("ben briggs", 230),
        ("carpool tunnel", 231),
        ("fractal dreamers", 234),
        ("onumi", 237),
        ("wisp x", 240),
        ("elfensjon", 251),
        ("kola kid", 253),
        ("pup", 256),
        ("ricky montgomery", 257),
        ("rin", 258),
        ("sound souler", 260),
        ("vinxis", 262),
        ("muzz", 284),
        ("maduk", 289),
        ("aitsuki nakuru", 290),
        ("ariabl'eyes", 294),
        ("omoi", 295),
        ("rohi", 309),
        ("in love with a ghost", 335),
    ];
    for &(artist_name, medal_id) in fa_packs {
        for (artist_key, distinct_set) in &artist_distinct_maps {
            if artist_key.contains(artist_name) && distinct_set.len() >= 3 {
                check_and_award(medal_id);
            }
        }
    }
    // Camellia II (ID 247) and Cranky II (ID 249) require 6+ maps
    for (artist_key, distinct_set) in &artist_distinct_maps {
        if (artist_key.contains("camellia") || artist_key.contains("かめりあ")) && distinct_set.len() >= 6 {
            check_and_award(247);
        }
        if artist_key.contains("cranky") && distinct_set.len() >= 6 {
            check_and_award(249);
        }
    }

    // Theme & Genre Pack tiers (3, 6, 10, 15 maps)
    let genre_pack_tiers: &[(&str, &[(usize, i32)])] = &[
        ("video_game", &[(3, 7), (6, 11), (10, 14), (15, 37)]),
        ("anime", &[(3, 10), (6, 12), (10, 25), (15, 34)]),
        ("internet", &[(3, 9), (6, 18), (10, 27), (15, 36)]),
        ("rhythm", &[(3, 8), (6, 19), (10, 26), (15, 35)]),
        ("touhou", &[(3, 282)]),
        ("vocaloid", &[(3, 288)]),
        ("dnb", &[(3, 310)]),
        ("chill", &[(3, 296)]),
    ];
    for &(genre_key, tiers) in genre_pack_tiers {
        if let Some(distinct_set) = genre_distinct_maps.get(genre_key) {
            for &(required_count, medal_id) in tiers {
                if distinct_set.len() >= required_count {
                    check_and_award(medal_id);
                }
            }
        }
    }

    if !newly_earned.is_empty() {
        let now = chrono::Utc::now().timestamp();
        let batch: Vec<(i32, i64)> = newly_earned.iter().map(|&id| (id, now)).collect();
        db::add_user_medals_batch(conn, player_name, &batch)?;
    }

    Ok(newly_earned)
}

/// Evaluates global rank milestones (ID 50..=53)
pub fn evaluate_rank_medals(rank: i32, existing: &HashSet<i32>) -> Vec<i32> {
    let mut newly_earned = Vec::new();
    if rank <= 0 || rank == 9999999 {
        return newly_earned;
    }
    let milestones = [
        (50_000, 50), // I Can See The Top (Top 50k)
        (10_000, 51), // The Gradual Rise (Top 10k)
        (5_000, 52),  // Scaling Up (Top 5k)
        (1_000, 53),  // Approaching The Summit (Top 1k)
    ];
    for &(thresh, id) in &milestones {
        if rank <= thresh && !existing.contains(&id) {
            newly_earned.push(id);
        }
    }
    newly_earned
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
        let medals = evaluate_score_submission(&s499, &bmap, None, 10, 0, &existing, true);
        assert!(!medals.contains(&1));

        // 500 combo -> awards ID 1
        let s500 = make_test_score(500, 0, true, 0, 500000);
        let medals = evaluate_score_submission(&s500, &bmap, None, 10, 0, &existing, true);
        assert!(medals.contains(&1));
        assert!(!medals.contains(&3));

        // 1000 combo -> awards ID 1, 3, 4
        let s1000 = make_test_score(1000, 0, true, 0, 1000000);
        let medals = evaluate_score_submission(&s1000, &bmap, None, 10, 0, &existing, true);
        assert!(medals.contains(&1));
        assert!(medals.contains(&3));
        assert!(medals.contains(&4));
        assert!(!medals.contains(&5));

        // When 1 and 3 already exist:
        let mut existing_with_some = HashSet::new();
        existing_with_some.insert(1);
        existing_with_some.insert(3);
        let medals = evaluate_score_submission(&s1000, &bmap, None, 10, 0, &existing_with_some, true);
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
        let medals = evaluate_score_submission(&pass_score, &bmap, None, 10, 0, &existing, true);
        assert!(medals.contains(&55)); // 1★ pass
        assert!(medals.contains(&59)); // 5★ pass
        assert!(!medals.contains(&60)); // 6★ pass
        assert!(!medals.contains(&63)); // 1★ FC should NOT be awarded

        // Score FC -> awards passes 1★–5★ AND FCs 1★–5★
        let fc_score = make_test_score(400, 0, true, 0, 800000);
        let medals = evaluate_score_submission(&fc_score, &bmap, None, 10, 0, &existing, true);
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
        let medals = evaluate_score_submission(&s, &bmap, None, 10, 0, &existing, true);
        assert!(medals.contains(&121)); // Rock Around The Clock (HR)
        assert!(medals.contains(&124)); // Blindsight (HD)
        assert!(medals.contains(&136)); // Afterimage (HD + HR)

        // DoubleTime
        let dt = Mods::DOUBLETIME.bits();
        let s_dt = make_test_score(100, 0, true, dt, 100000);
        let medals_dt = evaluate_score_submission(&s_dt, &bmap, None, 10, 0, &existing, true);
        assert!(medals_dt.contains(&122)); // Time And A Half (DT)

        // Relax / unranked mods should not award mod intro medals
        let rx = (Mods::DOUBLETIME | Mods::RELAX).bits();
        let s_rx = make_test_score(100, 0, true, rx, 100000);
        let medals_rx = evaluate_score_submission(&s_rx, &bmap, None, 10, 0, &existing, true);
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
        let medals = evaluate_score_submission(&jackpot_score, &bmap, None, 10, 0, &existing, true);
        assert!(medals.contains(&41)); // Jackpot

        // Consolation Prize: fail at >= 95% of map
        let mut fail_score = make_test_score(100, 1, false, 0, 300000);
        fail_score.n300 = 96; // 96 out of 100 objects hit
        let medals_fail = evaluate_score_submission(&fail_score, &bmap, None, 10, 0, &existing, false);
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

        // Should include playcount 5k (20), 500 combo (1), passes 1★–4★ (55..=58), FCs 1★–4★ (63..=66), HardRock (121), Challenge Accepted (39)
        assert!(new_medals.contains(&20));
        assert!(new_medals.contains(&1));
        assert!(new_medals.contains(&55));
        assert!(new_medals.contains(&58));
        assert!(new_medals.contains(&63));
        assert!(new_medals.contains(&66));
        assert!(new_medals.contains(&121));
        assert!(new_medals.contains(&39)); // Challenge Accepted (HR on >= 3.5★)

        // Re-running retroactive evaluation yields 0 new medals
        let rerun = retroactive_eval_profile(&conn, "RetroTester").unwrap();
        assert!(rerun.is_empty());

        // Verify count in database
        let count = db::get_user_medal_count(&conn, "RetroTester").unwrap();
        assert_eq!(count, new_medals.len());
    }

    #[test]
    fn test_multimode_passes_and_secrets() {
        let mut bmap = Beatmap::blank();
        bmap.difficultyrating = 4.2;
        bmap.hit_length = 25; // <= 30 -> Quick Draw (42)

        let existing = HashSet::new();

        // Taiko (mode 1) 4.2★ FC score with 35,000 cumulative hits
        let mut s_taiko = make_test_score(200, 0, true, 0, 400000);
        s_taiko.mode = 1;
        let medals_taiko = evaluate_score_submission(&s_taiko, &bmap, None, 10, 35000, &existing, true);
        assert!(medals_taiko.contains(&71)); // Taiko 1★ pass
        assert!(medals_taiko.contains(&74)); // Taiko 4★ pass
        assert!(!medals_taiko.contains(&75)); // Taiko 5★ pass
        assert!(medals_taiko.contains(&95)); // Taiko 1★ FC
        assert!(medals_taiko.contains(&98)); // Taiko 4★ FC
        assert!(medals_taiko.contains(&42)); // Quick Draw
        assert!(medals_taiko.contains(&31)); // 30,000 Drum Hits

        // Mania (mode 3) 4.2★ pass with D-rank (Stumbler) and 50,000 keys
        let mut s_mania = make_test_score(150, 5, false, 0, 200000);
        s_mania.mode = 3;
        s_mania.acc = Some(65.0); // Stumbler (40)
        let medals_mania = evaluate_score_submission(&s_mania, &bmap, None, 10, 50000, &existing, true);
        assert!(medals_mania.contains(&87)); // Mania 1★ pass
        assert!(medals_mania.contains(&90)); // Mania 4★ pass
        assert!(!medals_mania.contains(&111)); // No FC
        assert!(medals_mania.contains(&40)); // Stumbler
        assert!(medals_mania.contains(&46)); // 40,000 Keys

        // Iconic meme score: 727 PP / combo (When You See It - 287)
        let mut s_wysi = make_test_score(727, 0, true, 0, 727000);
        s_wysi.pp = Some(727.0);
        let medals_wysi = evaluate_score_submission(&s_wysi, &bmap, None, 10, 0, &existing, true);
        assert!(medals_wysi.contains(&287)); // When You See It

        // Natural 20 (ID 222)
        let s_nat20 = make_test_score(20, 0, true, 0, 50000);
        let medals_nat20 = evaluate_score_submission(&s_nat20, &bmap, None, 10, 0, &existing, true);
        assert!(medals_nat20.contains(&222)); // Natural 20

        // Dead Center (ID 276): >= 3.0* with equal circles and sliders
        let mut bmap_balanced = Beatmap::blank();
        bmap_balanced.difficultyrating = 3.5;
        bmap_balanced.count_normal = 100;
        bmap_balanced.count_slider = 100;
        let s_balanced = make_test_score(200, 0, true, 0, 100000);
        let medals_dc = evaluate_score_submission(&s_balanced, &bmap_balanced, None, 10, 0, &existing, true);
        assert!(medals_dc.contains(&276)); // Dead Center

        // Equilibrium (ID 159): 20x 300s, 20x 100s, 20x 50s
        let mut s_eq = make_test_score(60, 0, true, 0, 100000);
        s_eq.n300 = 20;
        s_eq.n100 = 20;
        s_eq.n50 = 20;
        let medals_eq = evaluate_score_submission(&s_eq, &bmap, None, 10, 0, &existing, true);
        assert!(medals_eq.contains(&159)); // Equilibrium

        // Eclipse (ID 139): Hidden + Flashlight
        let hd_fl = (Mods::HIDDEN | Mods::FLASHLIGHT).bits();
        let s_hdfl = make_test_score(100, 0, true, hd_fl, 100000);
        let medals_hdfl = evaluate_score_submission(&s_hdfl, &bmap, None, 10, 0, &existing, true);
        assert!(medals_hdfl.contains(&139)); // Eclipse

        // Lights Out (ID 144): Nightcore + Flashlight
        let nc_fl = (Mods::NIGHTCORE | Mods::FLASHLIGHT).bits();
        let s_ncfl = make_test_score(100, 0, true, nc_fl, 100000);
        let medals_ncfl = evaluate_score_submission(&s_ncfl, &bmap, None, 10, 0, &existing, true);
        assert!(medals_ncfl.contains(&144)); // Lights Out

        // Rank milestones evaluation
        let rank_medals = evaluate_rank_medals(450, &existing);
        assert!(rank_medals.contains(&50)); // Top 50,000
        assert!(rank_medals.contains(&51)); // Top 10,000
        assert!(rank_medals.contains(&52)); // Top 5,000
        assert!(rank_medals.contains(&53)); // Top 1,000

        // Ten To One (ID 268)
        let s_10 = make_test_score(10, 0, true, 0, 5000);
        let medals_10 = evaluate_score_submission(&s_10, &bmap, None, 10, 0, &existing, true);
        assert!(medals_10.contains(&268));

        // Up For The Challenge (ID 143): HD + HR + DT
        let hd_hr_dt = (Mods::HIDDEN | Mods::HARDROCK | Mods::DOUBLETIME).bits();
        let s_hdhrdt = make_test_score(100, 0, true, hd_hr_dt, 100000);
        let medals_hdhrdt = evaluate_score_submission(&s_hdhrdt, &bmap, None, 10, 0, &existing, true);
        assert!(medals_hdhrdt.contains(&143));

        // Don't let the bunny distract you! (ID 6)
        let mut bmap_bunny = Beatmap::blank();
        bmap_bunny.title = "Daydream cafe (TV Size)".to_string();
        let s_bunny = make_test_score(100, 0, true, 0, 100000);
        let medals_bunny = evaluate_score_submission(&s_bunny, &bmap_bunny, None, 10, 0, &existing, true);
        assert!(medals_bunny.contains(&6));

        // The Strongest Ice Fairy (ID 347)
        let mut bmap_cirno = Beatmap::blank();
        bmap_cirno.title = "Chirumiru Cirno".to_string();
        let s_cirno = make_test_score(100, 0, true, 0, 100000);
        let medals_cirno = evaluate_score_submission(&s_cirno, &bmap_cirno, None, 10, 0, &existing, true);
        assert!(medals_cirno.contains(&347));

        // Just One Second (ID 135) on early fail
        let mut bmap_fail = Beatmap::blank();
        bmap_fail.count_normal = 100;
        let mut s_early_fail = make_test_score(2, 1, false, 0, 1000);
        s_early_fail.n300 = 1;
        s_early_fail.nmiss = 1;
        let medals_early = evaluate_score_submission(&s_early_fail, &bmap_fail, None, 10, 0, &existing, false);
        assert!(medals_early.contains(&135));

        // True Torment (ID 173) on 99% fail
        let mut s_late_fail = make_test_score(99, 1, false, 0, 100000);
        s_late_fail.n300 = 99;
        s_late_fail.nmiss = 1;
        let medals_late = evaluate_score_submission(&s_late_fail, &bmap_fail, None, 10, 0, &existing, false);
        assert!(medals_late.contains(&173));
        assert!(medals_late.contains(&38)); // Also qualifies for Consolation Prize
    }
}

