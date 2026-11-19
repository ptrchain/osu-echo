use crate::multiplayer::bot::SkillProfile;
use crate::multiplayer::match_state::WinCondition;
use crate::packets::ScoreFrame;
use crate::types::mods::Mods;

#[derive(Debug, Clone)]
pub struct SimulationResult {
    pub timeline: Vec<ScoreFrame>,
    pub fail_time_ms: Option<i32>,
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
}
