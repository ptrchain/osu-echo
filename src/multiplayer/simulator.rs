use crate::multiplayer::bot::SkillProfile;
use crate::multiplayer::match_state::WinCondition;
use crate::packets::ScoreFrame;
use crate::types::mods::Mods;

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub timeline: Vec<ScoreFrame>,
    pub fail_time_ms: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstrainedScore {
    pub n300: u16,
    pub n100: u16,
    pub n50: u16,
    pub ngeki: u16,
    pub nkatu: u16,
    pub nmiss: u16,
    pub max_combo: u16,
    pub total_score: i32,
    pub mods: u32,
    pub perfect: bool,
}

/// Deterministic SplitMix64 PRNG for reproducible per-bot simulations.
#[derive(Debug, Clone)]
struct Rng {
    state: u64,
}

impl Rng {
    fn new(match_seed: u64, slot_idx: usize, bot_userid: i32, run_nonce: u64) -> Self {
        let mut s = match_seed
            .wrapping_mul(0x9E37_79B9_7F4A_7C15)
            .wrapping_add((slot_idx as u64 + 1).wrapping_mul(0xBF58_476D_1CE4_E5B9))
            .wrapping_add((bot_userid as u64).wrapping_mul(0x94D0_49BB_1331_11EB))
            .wrapping_add(run_nonce.wrapping_mul(0xD6E8_FEB8_6659_FD93));
        if s == 0 {
            s = 0x1234_5678_9ABC_DEF0;
        }
        Self { state: s }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1u64 << 53) as f64)
    }
}

struct SimObject {
    time_ms: i32,
    local_sr: f64,
}

struct ParsedSimMap {
    objects: Vec<SimObject>,
    od: f64,
    hp: f64,
    diff_multiplier: f64,
}

fn mod_score_multiplier(mods: Mods, mode: u8) -> f64 {
    let mut mult = 1.0;
    if mods.contains(Mods::EASY) {
        mult *= 0.5;
    }
    if mods.contains(Mods::NOFAIL) {
        mult *= 0.5;
    }
    if mods.contains(Mods::HALFTIME) {
        mult *= 0.3;
    }
    if mods.contains(Mods::SPUNOUT) {
        mult *= 0.9;
    }
    if mode != 3 {
        if mods.contains(Mods::HIDDEN) {
            mult *= 1.06;
        }
        if mods.contains(Mods::HARDROCK) {
            mult *= 1.06;
        }
        if mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) {
            mult *= 1.12;
        }
        if mods.contains(Mods::FLASHLIGHT) {
            mult *= 1.12;
        }
    }
    mult
}

fn to_game_mode(mode: u8) -> rosu_pp::model::mode::GameMode {
    match mode {
        1 => rosu_pp::model::mode::GameMode::Taiko,
        2 => rosu_pp::model::mode::GameMode::Catch,
        3 => rosu_pp::model::mode::GameMode::Mania,
        _ => rosu_pp::model::mode::GameMode::Osu,
    }
}

fn analyze_beatmap(osu_bytes: &[u8], mode: u8, effective_mods: u32) -> Option<ParsedSimMap> {
    let mut beatmap = rosu_pp::Beatmap::from_bytes(osu_bytes).ok()?;
    let target_mode = to_game_mode(mode);
    if beatmap.mode != target_mode {
        let _ = beatmap.convert_mut(target_mode, &rosu_pp::GameMods::from(effective_mods));
    }

    if beatmap.hit_objects.is_empty() {
        return None;
    }

    let diff = rosu_pp::Difficulty::new().lazer(false).mods(effective_mods).calculate(&beatmap);
    let overall_sr = diff.stars().max(0.5);

    let mods = Mods::from_bits_truncate(effective_mods);
    let mut od = beatmap.od as f64;
    let mut hp = beatmap.hp as f64;
    let mut cs = beatmap.cs as f64;
    if mods.contains(Mods::HARDROCK) {
        od = (od * 1.4).min(10.0);
        hp = (hp * 1.4).min(10.0);
        cs = (cs * 1.3).min(10.0);
    } else if mods.contains(Mods::EASY) {
        od *= 0.5;
        hp *= 0.5;
        cs *= 0.5;
    }

    let n = beatmap.hit_objects.len();
    let raw_objs: Vec<(i32, f32, f32)> = beatmap
        .hit_objects
        .iter()
        .map(|ho| (ho.start_time.round() as i32, ho.pos.x, ho.pos.y))
        .collect();

    // Compute local intensity per object using density (within ±750ms) and jump distance
    let mut raw_weights = Vec::with_capacity(n);
    let mut left = 0usize;
    let mut right = 0usize;
    for i in 0..n {
        let t = raw_objs[i].0;
        while left < i && t - raw_objs[left].0 > 750 {
            left += 1;
        }
        while right + 1 < n && raw_objs[right + 1].0 - t <= 750 {
            right += 1;
        }
        let density = (right - left + 1) as f64;
        let spacing = if i > 0 && mode == 0 {
            let dx = (raw_objs[i].1 - raw_objs[i - 1].1) as f64;
            let dy = (raw_objs[i].2 - raw_objs[i - 1].2) as f64;
            let dt = (t - raw_objs[i - 1].0).max(20) as f64;
            (dx * dx + dy * dy).sqrt() / dt * 100.0
        } else {
            0.0
        };
        raw_weights.push(density + spacing * 0.35);
    }

    let mean_weight = (raw_weights.iter().sum::<f64>() / n as f64).max(1.0);
    let objects = raw_objs
        .into_iter()
        .zip(raw_weights)
        .map(|((time_ms, _, _), w)| {
            let ratio = (w / mean_weight).clamp(0.55, 1.55);
            SimObject {
                time_ms,
                local_sr: (overall_sr * ratio).max(0.3),
            }
        })
        .collect();

    let diff_multiplier = ((hp + cs + od) / 3.0).clamp(1.0, 10.0).round();

    Some(ParsedSimMap {
        objects,
        od,
        hp,
        diff_multiplier,
    })
}

fn fallback_map(fallback_sr: f64) -> ParsedSimMap {
    let count = 180usize;
    let duration_ms = 120_000i32;
    let step = duration_ms / count as i32;
    let objects = (0..count)
        .map(|i| {
            let progress = i as f64 / count as f64;
            // Gentle diffspike around 65% of the song
            let spike = 0.8 + 0.5 * (-((progress - 0.65) / 0.15).powi(2)).exp();
            SimObject {
                time_ms: 1_500 + i as i32 * step,
                local_sr: (fallback_sr * spike).max(0.5),
            }
        })
        .collect();

    ParsedSimMap {
        objects,
        od: 7.5,
        hp: 5.5,
        diff_multiplier: 5.0,
    }
}

pub fn simulate_bot(
    osu_bytes: Option<&[u8]>,
    skill: &SkillProfile,
    mode: u8,
    effective_mods: u32,
    win_condition: WinCondition,
    slot_idx: usize,
    bot_userid: i32,
    match_seed: u64,
    run_nonce: u64,
) -> SimulationResult {
    let map = osu_bytes
        .and_then(|b| analyze_beatmap(b, mode, effective_mods))
        .unwrap_or_else(|| fallback_map(5.0));

    let mods = Mods::from_bits_truncate(effective_mods);
    let has_nf = mods.contains(Mods::NOFAIL);
    let has_sd = mods.contains(Mods::SUDDENDEATH);
    let has_pf = mods.contains(Mods::PERFECT);
    let using_score_v2 = win_condition == WinCondition::ScoreV2 || mods.contains(Mods::SCOREV2);
    let mod_mult = mod_score_multiplier(mods, mode);
    let total_objects = map.objects.len().max(1);

    let mut rng = Rng::new(match_seed, slot_idx, bot_userid, run_nonce);

    // Slight per-run form variance so repeated runs feel natural
    let form_factor = 0.94 + rng.next_f64() * 0.12;
    let eff_target_sr = (skill.target_sr * form_factor).max(0.5);
    let base_acc = (skill.accuracy_mean / 100.0).clamp(0.5, 1.0);
    let consistency = skill.consistency.clamp(0.0, 1.0);
    let choke = skill.choke_tendency.clamp(0.0, 1.0);

    let mut n300: u16 = 0;
    let mut n100: u16 = 0;
    let mut n50: u16 = 0;
    let mut ngeki: u16 = 0;
    let mut nkatu: u16 = 0;
    let mut nmiss: u16 = 0;
    let mut current_combo: u16 = 0;
    let mut max_combo: u16 = 0;
    let mut perfect = true;
    let mut score_v1: f64 = 0.0;
    let mut mania_bonus: f64 = 100.0;
    let mut mania_score: f64 = 0.0;
    let mut hp: f64 = 200.0;
    let mut fail_time_ms: Option<i32> = None;

    let mut timeline: Vec<ScoreFrame> = Vec::with_capacity(map.objects.len() / 4 + 2);
    let mut last_emit_t: Option<i32> = None;
    let mut prev_t: i32 = map.objects.first().map(|o| o.time_ms).unwrap_or(0);

    // Precompute max possible combo-sum for ScoreV2 combo portion
    let max_combo_sum = (total_objects as f64) * ((total_objects as f64) + 1.0) / 2.0;
    let mut v2_combo_sum: f64 = 0.0;

    for (idx, obj) in map.objects.iter().enumerate() {
        let dt_sec = ((obj.time_ms - prev_t).max(0) as f64) / 1000.0;
        prev_t = obj.time_ms;

        // Passive HP drain (clamped per step so long breaks don't kill bots)
        let drain = (map.hp * 2.2 * dt_sec.min(1.5)).max(0.0);
        hp = (hp - drain).clamp(0.0, 200.0);

        let pressure = (obj.local_sr / eff_target_sr).max(0.1);
        let progress = idx as f64 / total_objects as f64;

        // Miss probability: very low when below skill, ramps up sharply above skill
        let excess = (pressure - 0.88).max(0.0);
        let choke_boost = if progress > 0.65 && current_combo as f64 > (total_objects as f64 * 0.4) {
            choke * 0.025 * pressure
        } else {
            0.0
        };
        let mut p_miss = (excess.powf(1.85) * (0.28 - 0.16 * consistency) + choke_boost).clamp(0.0, 0.72);
        if pressure < 0.75 {
            p_miss *= 0.25;
        }

        // Non-300 probability driven by target accuracy, OD, and pressure
        let od_factor = 0.75 + (map.od / 10.0) * 0.5;
        let acc_error = (1.0 - base_acc).max(0.002) * od_factor * (0.65 + 0.55 * pressure);
        let p_100 = (acc_error * 1.15).clamp(0.0, (1.0 - p_miss).max(0.0) * 0.85);
        let p_50 = (acc_error * 0.22 * (1.2 - consistency * 0.6)).clamp(0.0, (1.0 - p_miss - p_100).max(0.0));

        let roll = rng.next_f64();
        let mut failed_now = false;

        if roll < p_miss {
            nmiss = nmiss.saturating_add(1);
            current_combo = 0;
            perfect = false;
            hp = (hp - (12.0 + map.hp * 4.5)).max(0.0);
            if mode == 3 {
                mania_bonus = (mania_bonus - 44.0).clamp(0.0, 100.0);
            }
            if skill.can_fail && !has_nf && (hp <= 0.0 || has_sd || has_pf) {
                hp = 0.0;
                failed_now = true;
            }
        } else if roll < p_miss + p_50 {
            n50 = n50.saturating_add(1);
            current_combo = current_combo.saturating_add(1);
            max_combo = max_combo.max(current_combo);
            if has_pf && skill.can_fail && !has_nf {
                hp = 0.0;
                failed_now = true;
            } else {
                hp = (hp + 0.5).min(200.0);
            }
            let hit_val = 50.0;
            score_v1 += hit_val + (hit_val * ((current_combo.saturating_sub(1) as f64) * map.diff_multiplier * mod_mult) / 25.0);
            v2_combo_sum += current_combo as f64;
            if mode == 3 {
                mania_bonus = (mania_bonus - 24.0).clamp(0.0, 100.0);
                mania_score += (500_000.0 * mod_mult / total_objects as f64) * (50.0 / 320.0)
                    + (500_000.0 * mod_mult / total_objects as f64) * (4.0 * mania_bonus.sqrt() / 320.0);
            }
        } else if roll < p_miss + p_50 + p_100 {
            n100 = n100.saturating_add(1);
            if mode == 3 && rng.next_f64() < 0.5 {
                nkatu = nkatu.saturating_add(1);
            }
            current_combo = current_combo.saturating_add(1);
            max_combo = max_combo.max(current_combo);
            if has_pf && skill.can_fail && !has_nf {
                hp = 0.0;
                failed_now = true;
            } else {
                hp = (hp + (2.5 - map.hp * 0.12).max(0.8)).min(200.0);
            }
            let hit_val = 100.0;
            score_v1 += hit_val + (hit_val * ((current_combo.saturating_sub(1) as f64) * map.diff_multiplier * mod_mult) / 25.0);
            v2_combo_sum += current_combo as f64;
            if mode == 3 {
                mania_bonus = (mania_bonus - 8.0).clamp(0.0, 100.0);
                mania_score += (500_000.0 * mod_mult / total_objects as f64) * (100.0 / 320.0)
                    + (500_000.0 * mod_mult / total_objects as f64) * (8.0 * mania_bonus.sqrt() / 320.0);
            }
        } else {
            if mode == 3 && rng.next_f64() < (0.55 + 0.35 * consistency) {
                ngeki = ngeki.saturating_add(1);
                mania_bonus = (mania_bonus + 2.0).clamp(0.0, 100.0);
                mania_score += (500_000.0 * mod_mult / total_objects as f64)
                    + (500_000.0 * mod_mult / total_objects as f64) * (32.0 * mania_bonus.sqrt() / 320.0);
            } else {
                n300 = n300.saturating_add(1);
                if mode == 3 {
                    mania_bonus = (mania_bonus + 1.0).clamp(0.0, 100.0);
                    mania_score += (500_000.0 * mod_mult / total_objects as f64) * (300.0 / 320.0)
                        + (500_000.0 * mod_mult / total_objects as f64) * (32.0 * mania_bonus.sqrt() / 320.0);
                }
            }
            current_combo = current_combo.saturating_add(1);
            max_combo = max_combo.max(current_combo);
            hp = (hp + (4.5 - map.hp * 0.22).max(1.5)).min(200.0);
            let hit_val = 300.0;
            score_v1 += hit_val + (hit_val * ((current_combo.saturating_sub(1) as f64) * map.diff_multiplier * mod_mult) / 25.0);
            v2_combo_sum += current_combo as f64;
        }

        let is_last = idx + 1 == map.objects.len();
        let interval_elapsed = last_emit_t.is_none_or(|lt| obj.time_ms.saturating_sub(lt) >= 1000);
        if failed_now || is_last || interval_elapsed {
            let (total_score, combo_portion, bonus_portion) = if mode == 3 {
                let capped = mania_score.round().clamp(0.0, 1_000_000.0) as i32;
                (capped, 0.0, 0.0)
            } else if using_score_v2 {
                let hits_so_far = (n300 as f64 + n100 as f64 + n50 as f64 + nmiss as f64).max(1.0);
                let acc_ratio = (n300 as f64 * 300.0 + n100 as f64 * 100.0 + n50 as f64 * 50.0) / (hits_so_far * 300.0);
                let c_part = (v2_combo_sum / max_combo_sum.max(1.0)).clamp(0.0, 1.0);
                let a_part = acc_ratio.powf(10.0) * (hits_so_far / total_objects as f64);
                let raw_v2 = (700_000.0 * c_part + 300_000.0 * a_part) * mod_mult;
                (raw_v2.round().clamp(0.0, 1_000_000.0) as i32, c_part, a_part)
            } else {
                (score_v1.round().clamp(0.0, i32::MAX as f64) as i32, 0.0, 0.0)
            };

            let frame = ScoreFrame {
                time: obj.time_ms,
                id: slot_idx as u8,
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
                current_hp: hp.round().clamp(0.0, 200.0) as u8,
                tag_byte: 0,
                using_score_v2,
                combo_portion,
                bonus_portion,
            };

            // Keep timestamps strictly monotonic
            if let Some(last) = timeline.last_mut() {
                if last.time == frame.time {
                    *last = frame;
                } else {
                    timeline.push(frame);
                }
            } else {
                timeline.push(frame);
            }
            last_emit_t = Some(obj.time_ms);
        }

        if failed_now {
            fail_time_ms = Some(obj.time_ms);
            break;
        }
    }

    SimulationResult { timeline, fail_time_ms }
}

/// Generates a realistic timestamped `ScoreFrame` timeline constrained to land on an exact
/// target score's `(n300, n100, n50, ngeki, nkatu, nmiss, max_combo, total_score)`.
pub fn simulate_constrained_score(
    osu_bytes: Option<&[u8]>,
    target: &ConstrainedScore,
    mode: u8,
    slot_idx: usize,
    match_seed: u64,
) -> SimulationResult {
    let map = osu_bytes
        .and_then(|b| analyze_beatmap(b, mode, target.mods))
        .unwrap_or_else(|| fallback_map(5.0));

    let is_mania = mode == 3;
    let total_hits: usize = if is_mania {
        target.ngeki as usize
            + target.n300 as usize
            + target.nkatu as usize
            + target.n100 as usize
            + target.n50 as usize
            + target.nmiss as usize
    } else {
        target.n300 as usize + target.n100 as usize + target.n50 as usize + target.nmiss as usize
    };

    if total_hits == 0 {
        let t = map.objects.last().map(|o| o.time_ms).unwrap_or(1_000);
        return SimulationResult {
            timeline: vec![ScoreFrame {
                time: t,
                id: slot_idx as u8,
                total_score: target.total_score,
                max_combo: target.max_combo,
                current_combo: target.max_combo,
                perfect: target.perfect,
                current_hp: 200,
                ..Default::default()
            }],
            fail_time_ms: None,
        };
    }

    let mut rng = Rng::new(match_seed, slot_idx, target.total_score, total_hits as u64);

    // Map each hit index to a beatmap timestamp and local_sr
    let map_len = map.objects.len().max(1);
    let mut times = Vec::with_capacity(total_hits);
    let mut strains = Vec::with_capacity(total_hits);
    for i in 0..total_hits {
        let src_idx = (i * map_len) / total_hits;
        let obj = &map.objects[src_idx.min(map_len - 1)];
        times.push(obj.time_ms);
        strains.push(obj.local_sr);
    }

    // Judgement codes: 0=miss, 1=50, 2=100, 3=300, 4=katu(mania), 5=geki(mania)
    let mut judgements = vec![3u8; total_hits];

    // Place misses biased toward highest-strain positions outside a protected streak
    let nmiss = (target.nmiss as usize).min(total_hits);
    if nmiss > 0 {
        let non_miss = total_hits - nmiss;
        let desired_streak = (target.max_combo as usize).min(non_miss);

        // Find lowest-strain window of length `desired_streak` to protect for max_combo
        let mut best_start = 0usize;
        if desired_streak > 0 && desired_streak < total_hits {
            let mut window_sum: f64 = strains[..desired_streak].iter().sum();
            let mut best_sum = window_sum;
            for start in 1..=(total_hits - desired_streak) {
                window_sum += strains[start + desired_streak - 1] - strains[start - 1];
                if window_sum < best_sum {
                    best_sum = window_sum;
                    best_start = start;
                }
            }
        }
        let streak_end = best_start + desired_streak;

        // Rank candidate indices outside [best_start, streak_end) by strain (+ small noise)
        let mut candidates: Vec<(usize, f64)> = (0..total_hits)
            .filter(|&i| i < best_start || i >= streak_end)
            .map(|i| (i, strains[i] + rng.next_f64() * 0.35))
            .collect();
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for &(idx, _) in candidates.iter().take(nmiss) {
            judgements[idx] = 0;
        }
    }

    // Rank remaining non-miss indices by strain (+ noise) to assign 50s, 100s, (and katu/geki in mania)
    let mut hit_indices: Vec<(usize, f64)> = (0..total_hits)
        .filter(|&i| judgements[i] != 0)
        .map(|i| (i, strains[i] + rng.next_f64() * 0.5))
        .collect();
    hit_indices.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

    let mut cursor = 0usize;
    for _ in 0..target.n50 as usize {
        if cursor < hit_indices.len() {
            judgements[hit_indices[cursor].0] = 1;
            cursor += 1;
        }
    }
    for _ in 0..target.n100 as usize {
        if cursor < hit_indices.len() {
            judgements[hit_indices[cursor].0] = 2;
            cursor += 1;
        }
    }
    if is_mania {
        for _ in 0..target.nkatu as usize {
            if cursor < hit_indices.len() {
                judgements[hit_indices[cursor].0] = 4;
                cursor += 1;
            }
        }
        // Assign geki to the easiest remaining hits (from the back)
        let mut back = hit_indices.len();
        for _ in 0..target.ngeki as usize {
            if back > cursor {
                back -= 1;
                judgements[hit_indices[back].0] = 5;
            }
        }
    }

    // First pass: compute raw combo & raw score curve for exact scaling
    let mut raw_combo = 0u32;
    let mut raw_max_combo = 0u32;
    let mut raw_scores = Vec::with_capacity(total_hits);
    let mut acc_raw_score = 0.0f64;
    for &j in &judgements {
        if j == 0 {
            raw_combo = 0;
        } else {
            raw_combo += 1;
            raw_max_combo = raw_max_combo.max(raw_combo);
            let base = match j {
                1 => 50.0,
                2 => 100.0,
                4 => 200.0,
                5 => 320.0,
                _ => 300.0,
            };
            acc_raw_score += base * (1.0 + (raw_combo as f64) * 0.04);
        }
        raw_scores.push(acc_raw_score);
    }

    let score_scale = if acc_raw_score > 0.0 {
        target.total_score as f64 / acc_raw_score
    } else {
        0.0
    };
    let combo_scale = if raw_max_combo > 0 {
        target.max_combo as f64 / raw_max_combo as f64
    } else {
        1.0
    };

    // Second pass: emit ~1000ms monotonic ScoreFrames landing on exact target values
    let mut n300: u16 = 0;
    let mut n100: u16 = 0;
    let mut n50: u16 = 0;
    let mut ngeki: u16 = 0;
    let mut nkatu: u16 = 0;
    let mut nmiss_so_far: u16 = 0;
    let mut cur_raw_combo = 0u32;
    let mut max_raw_combo_so_far = 0u32;
    let mut hp: f64 = 200.0;
    let mut last_emit_t: Option<i32> = None;
    let mut timeline: Vec<ScoreFrame> = Vec::with_capacity(total_hits / 4 + 2);
    let using_score_v2 = Mods::from_bits_truncate(target.mods).contains(Mods::SCOREV2);

    for i in 0..total_hits {
        let j = judgements[i];
        match j {
            0 => {
                nmiss_so_far = nmiss_so_far.saturating_add(1);
                cur_raw_combo = 0;
                hp = (hp - 18.0).max(20.0);
            }
            1 => {
                n50 = n50.saturating_add(1);
                cur_raw_combo += 1;
                hp = (hp + 1.0).min(200.0);
            }
            2 => {
                n100 = n100.saturating_add(1);
                cur_raw_combo += 1;
                hp = (hp + 2.5).min(200.0);
            }
            4 => {
                nkatu = nkatu.saturating_add(1);
                cur_raw_combo += 1;
                hp = (hp + 3.5).min(200.0);
            }
            5 => {
                ngeki = ngeki.saturating_add(1);
                cur_raw_combo += 1;
                hp = (hp + 4.5).min(200.0);
            }
            _ => {
                n300 = n300.saturating_add(1);
                cur_raw_combo += 1;
                hp = (hp + 4.0).min(200.0);
            }
        }
        max_raw_combo_so_far = max_raw_combo_so_far.max(cur_raw_combo);

        let is_last = i + 1 == total_hits;
        let t_ms = times[i];
        let interval_elapsed = last_emit_t.is_none_or(|lt| t_ms.saturating_sub(lt) >= 1000);

        if is_last || interval_elapsed {
            let scaled_score = if is_last {
                target.total_score
            } else {
                (raw_scores[i] * score_scale).round().clamp(0.0, target.total_score as f64) as i32
            };
            let scaled_max_combo = if is_last {
                target.max_combo
            } else {
                (max_raw_combo_so_far as f64 * combo_scale).round().clamp(0.0, target.max_combo as f64) as u16
            };
            let scaled_cur_combo = (cur_raw_combo as f64 * combo_scale).round().clamp(0.0, target.max_combo as f64) as u16;
            let progress = (i + 1) as f64 / total_hits as f64;
            let (out_geki, out_katu) = if is_mania {
                (ngeki, nkatu)
            } else if is_last {
                (target.ngeki, target.nkatu)
            } else {
                (
                    (target.ngeki as f64 * progress).round() as u16,
                    (target.nkatu as f64 * progress).round() as u16,
                )
            };

            let frame = ScoreFrame {
                time: t_ms,
                id: slot_idx as u8,
                n300,
                n100,
                n50,
                ngeki: out_geki,
                nkatu: out_katu,
                nmiss: nmiss_so_far,
                total_score: scaled_score,
                max_combo: scaled_max_combo,
                current_combo: scaled_cur_combo,
                perfect: if is_last { target.perfect } else { nmiss_so_far == 0 },
                current_hp: hp.round().clamp(1.0, 200.0) as u8,
                tag_byte: 0,
                using_score_v2,
                combo_portion: 0.0,
                bonus_portion: 0.0,
            };

            if let Some(last) = timeline.last_mut() {
                if last.time >= frame.time {
                    let mut fixed = frame;
                    fixed.time = last.time;
                    *last = fixed;
                } else {
                    timeline.push(frame);
                }
            } else {
                timeline.push(frame);
            }
            last_emit_t = Some(t_ms);
        }
    }

    SimulationResult {
        timeline,
        fail_time_ms: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_osu_map(hp: f32, spacing: i32, step_ms: i32, count: usize) -> Vec<u8> {
        let mut s = format!(
            "osu file format v14\n\n\
            [General]\n\
            AudioFilename: audio.mp3\n\
            Mode: 0\n\n\
            [Metadata]\n\
            Title: TestMap\n\
            Artist: TestArtist\n\
            Version: Diff\n\n\
            [Difficulty]\n\
            HPDrainRate:{hp}\n\
            CircleSize:4\n\
            OverallDifficulty:8\n\
            ApproachRate:9\n\
            SliderMultiplier:1.4\n\
            SliderTickRate:1\n\n\
            [TimingPoints]\n\
            0,300,4,1,0,100,1,0\n\n\
            [HitObjects]\n"
        );
        for i in 0..count {
            let x = 100 + ((i as i32 * spacing) % 320);
            let y = 100 + (((i as i32 / 2) * spacing) % 240);
            let t = 1000 + i as i32 * step_ms;
            s.push_str(&format!("{x},{y},{t},1,0,0:0:0:0:\n"));
        }
        s.into_bytes()
    }

    #[test]
    fn test_higher_sr_bot_outscores_lower_sr_bot() {
        let map = sample_osu_map(5.0, 120, 160, 120);
        let low_skill = SkillProfile {
            target_sr: 2.0,
            accuracy_mean: 90.0,
            consistency: 0.45,
            choke_tendency: 0.3,
            preferred_mods: 0,
            can_fail: false,
        };
        let high_skill = SkillProfile {
            target_sr: 9.0,
            accuracy_mean: 99.3,
            consistency: 0.94,
            choke_tendency: 0.05,
            preferred_mods: 0,
            can_fail: false,
        };

        let low_res = simulate_bot(Some(&map), &low_skill, 0, 0, WinCondition::Score, 1, 1_000_000_001, 42, 0);
        let high_res = simulate_bot(Some(&map), &high_skill, 0, 0, WinCondition::Score, 2, 1_000_000_002, 42, 0);

        let low_final = low_res.timeline.last().unwrap();
        let high_final = high_res.timeline.last().unwrap();
        assert!(high_final.total_score > low_final.total_score, "high={} should beat low={}", high_final.total_score, low_final.total_score);
        assert_eq!(low_final.n300 + low_final.n100 + low_final.n50 + low_final.nmiss, 120);
        assert_eq!(high_final.n300 + high_final.n100 + high_final.n50 + high_final.nmiss, 120);
    }

    #[test]
    fn test_identical_presets_in_different_slots_produce_distinct_timelines() {
        let map = sample_osu_map(4.0, 100, 180, 100);
        let skill = SkillProfile::default();
        let r1 = simulate_bot(Some(&map), &skill, 0, 0, WinCondition::Score, 1, 1_000_000_001, 777, 0);
        let r2 = simulate_bot(Some(&map), &skill, 0, 0, WinCondition::Score, 2, 1_000_000_002, 777, 0);
        assert_ne!(r1.timeline, r2.timeline);
    }

    #[test]
    fn test_score_v2_and_mania_caps_and_v1_mod_scaling() {
        let map = sample_osu_map(4.0, 80, 200, 100);
        let pro = SkillProfile {
            target_sr: 10.0,
            accuracy_mean: 99.5,
            consistency: 0.96,
            choke_tendency: 0.0,
            preferred_mods: 0,
            can_fail: false,
        };

        // ScoreV2 stays <= 1,000,000
        let v2 = simulate_bot(Some(&map), &pro, 0, 0, WinCondition::ScoreV2, 1, 1_000_000_001, 99, 0);
        let v2_final = v2.timeline.last().unwrap();
        assert!(v2_final.using_score_v2);
        assert!(v2_final.total_score <= 1_000_000 && v2_final.total_score > 0);

        // Mania stays <= 1,000,000
        let mania = simulate_bot(Some(&map), &pro, 3, 0, WinCondition::Score, 1, 1_000_000_001, 99, 0);
        let mania_final = mania.timeline.last().unwrap();
        assert!(mania_final.total_score <= 1_000_000 && mania_final.total_score > 0);

        // ScoreV1 scales with HR+HD+DT vs NM
        let nm = simulate_bot(Some(&map), &pro, 0, 0, WinCondition::Score, 1, 1_000_000_001, 99, 0);
        let hdhrdt = simulate_bot(
            Some(&map),
            &pro,
            0,
            (Mods::HIDDEN | Mods::HARDROCK | Mods::DOUBLETIME).bits(),
            WinCondition::Score,
            1,
            1_000_000_001,
            99,
            0,
        );
        assert!(hdhrdt.timeline.last().unwrap().total_score > nm.timeline.last().unwrap().total_score);
    }

    #[test]
    fn test_fail_behavior_and_nofail_protection() {
        // High HP + dense jumps -> low-skill bot fails
        let hard_map = sample_osu_map(9.5, 180, 100, 150);
        let weak = SkillProfile {
            target_sr: 1.5,
            accuracy_mean: 80.0,
            consistency: 0.2,
            choke_tendency: 0.5,
            preferred_mods: 0,
            can_fail: true,
        };

        let res_fail = simulate_bot(Some(&hard_map), &weak, 0, 0, WinCondition::Score, 1, 1_000_000_001, 123, 0);
        assert!(res_fail.fail_time_ms.is_some(), "weak bot should fail on high-HP hard map");
        assert_eq!(res_fail.timeline.last().unwrap().current_hp, 0);

        let res_nf = simulate_bot(Some(&hard_map), &weak, 0, Mods::NOFAIL.bits(), WinCondition::Score, 1, 1_000_000_001, 123, 0);
        assert!(res_nf.fail_time_ms.is_none(), "NF must prevent failing");
    }

    #[test]
    fn test_strictly_monotonic_timestamps_and_fallback() {
        let skill = SkillProfile::default();
        let res = simulate_bot(None, &skill, 0, 0, WinCondition::Score, 1, 1_000_000_001, 55, 0);
        assert!(!res.timeline.is_empty());
        for w in res.timeline.windows(2) {
            assert!(w[1].time > w[0].time, "timestamps must be strictly monotonic: {} -> {}", w[0].time, w[1].time);
        }
    }

    #[test]
    fn test_simulate_constrained_score_exact_final_frame() {
        let map = sample_osu_map(5.0, 120, 150, 150);
        let target = ConstrainedScore {
            n300: 132,
            n100: 11,
            n50: 3,
            ngeki: 24,
            nkatu: 5,
            nmiss: 4,
            max_combo: 95,
            total_score: 2_345_678,
            mods: (Mods::HIDDEN | Mods::DOUBLETIME).bits(),
            perfect: false,
        };

        let res = simulate_constrained_score(Some(&map), &target, 0, 2, 1234);
        assert!(res.fail_time_ms.is_none());
        assert!(res.timeline.len() >= 2);
        for w in res.timeline.windows(2) {
            assert!(w[1].time > w[0].time);
            assert!(w[1].total_score >= w[0].total_score);
        }

        let last = res.timeline.last().unwrap();
        assert_eq!(last.id, 2);
        assert_eq!(last.n300, 132);
        assert_eq!(last.n100, 11);
        assert_eq!(last.n50, 3);
        assert_eq!(last.ngeki, 24);
        assert_eq!(last.nkatu, 5);
        assert_eq!(last.nmiss, 4);
        assert_eq!(last.max_combo, 95);
        assert_eq!(last.total_score, 2_345_678);
        assert!(!last.perfect);
    }
}
