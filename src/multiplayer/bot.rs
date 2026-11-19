use crate::types::mods::Mods;
use crate::types::player::Player;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
pub struct SkillProfile {
    pub target_sr: f64,
    pub accuracy_mean: f64,
    pub consistency: f64,
    pub choke_tendency: f64,
    pub preferred_mods: u32,
    pub can_fail: bool,
}

impl Default for SkillProfile {
    fn default() -> Self {
        Self {
            target_sr: 5.3,
            accuracy_mean: 97.5,
            consistency: 0.72,
            choke_tendency: 0.25,
            preferred_mods: 0,
            can_fail: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BotSource {
    Procedural(SkillProfile),
    MimicPlayer {
        username: String,
        osu_user_id: Option<i32>,
        fallback_skill: SkillProfile,
    },
    PersonalBest,
    ScoreGhost(crate::multiplayer::simulator::ConstrainedScore),
    ReplayGhost {
        replay_path: PathBuf,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct BotProfile {
    pub userid: i32,
    pub name: String,
    pub country: u8,
    pub rank: i32,
    pub pp: i32,
    pub acc: f64,
    pub source: BotSource,
}

/// Estimates a comfortable target star rating from a player's total PP.
pub fn infer_target_sr_from_pp(pp: i32) -> f64 {
    if pp <= 0 {
        return 3.0;
    }
    let sr = 2.0 + 0.052 * (pp as f64).powf(0.495);
    sr.clamp(2.0, 12.0)
}

impl BotProfile {
    pub fn from_preset(preset: &str, userid: i32) -> Option<Self> {
        let (name, rank, pp, skill) = match preset.to_ascii_lowercase().as_str() {
            "6digit" => (
                "Bot [6-digit]",
                350_000,
                950,
                SkillProfile {
                    target_sr: 3.5,
                    accuracy_mean: 94.5,
                    consistency: 0.55,
                    choke_tendency: 0.35,
                    preferred_mods: 0,
                    can_fail: true,
                },
            ),
            "5digit" => (
                "Bot [5-digit]",
                45_000,
                4_200,
                SkillProfile {
                    target_sr: 5.3,
                    accuracy_mean: 97.5,
                    consistency: 0.72,
                    choke_tendency: 0.25,
                    preferred_mods: 0,
                    can_fail: true,
                },
            ),
            "4digit" => (
                "Bot [4-digit]",
                4_500,
                8_200,
                SkillProfile {
                    target_sr: 6.5,
                    accuracy_mean: 98.5,
                    consistency: 0.82,
                    choke_tendency: 0.18,
                    preferred_mods: 0,
                    can_fail: true,
                },
            ),
            "3digit" => (
                "Bot [3-digit]",
                450,
                13_500,
                SkillProfile {
                    target_sr: 7.6,
                    accuracy_mean: 99.0,
                    consistency: 0.88,
                    choke_tendency: 0.12,
                    preferred_mods: 0,
                    can_fail: true,
                },
            ),
            "2digit" => (
                "Bot [2-digit]",
                45,
                18_500,
                SkillProfile {
                    target_sr: 8.6,
                    accuracy_mean: 99.2,
                    consistency: 0.92,
                    choke_tendency: 0.08,
                    preferred_mods: 0,
                    can_fail: true,
                },
            ),
            "1digit" => (
                "Bot [1-digit]",
                5,
                24_000,
                SkillProfile {
                    target_sr: 9.5,
                    accuracy_mean: 99.4,
                    consistency: 0.95,
                    choke_tendency: 0.05,
                    preferred_mods: 0,
                    can_fail: true,
                },
            ),
            _ => return None,
        };

        Some(Self {
            userid,
            name: name.to_string(),
            country: 0,
            rank,
            pp,
            acc: skill.accuracy_mean,
            source: BotSource::Procedural(skill),
        })
    }

    pub fn rival_for(player: &Player, userid: i32) -> Self {
        let effective_pp = if player.pp > 0 { player.pp } else { 1_500 };
        let target_sr = infer_target_sr_from_pp(effective_pp);
        let acc = if player.acc > 0.0 {
            player.acc.clamp(75.0, 99.8)
        } else {
            96.5
        };
        let rank = if player.rank > 0 && player.rank < 9_999_999 {
            player.rank
        } else {
            100_000
        };

        Self {
            userid,
            name: "Rival Bot".to_string(),
            country: player.country,
            rank,
            pp: player.pp,
            acc,
            source: BotSource::Procedural(SkillProfile {
                target_sr,
                accuracy_mean: acc,
                consistency: 0.78,
                choke_tendency: 0.20,
                preferred_mods: 0,
                can_fail: true,
            }),
        }
    }

    pub fn preferred_mods(&self) -> u32 {
        match &self.source {
            BotSource::Procedural(s) => s.preferred_mods,
            BotSource::MimicPlayer { fallback_skill, .. } => fallback_skill.preferred_mods,
            BotSource::ScoreGhost(sc) => sc.mods,
            BotSource::PersonalBest | BotSource::ReplayGhost { .. } => 0,
        }
    }

    pub fn to_bancho_player(&self, mode: u8) -> Player {
        let mut p = Player::new(self.name.clone());
        p.userid = self.userid;
        p.bancho_privs = 1;
        p.mode = mode;
        p.country = self.country;
        p.rank = self.rank;
        p.pp = self.pp;
        p.acc = self.acc;
        p.action = 0;
        p
    }
}

/// Resolves mutually exclusive osu! mod flags so `rosu-pp` and the client never receive invalid combos.
fn resolve_mod_conflicts(mut mods: Mods) -> Mods {
    // Strip unplayable/non-multiplayer mods
    mods.remove(Mods::AUTOPLAY | Mods::CINEMA | Mods::TARGET);

    // Speed conflicts: DT/NC beats HT
    if mods.intersects(Mods::DOUBLETIME | Mods::NIGHTCORE) && mods.contains(Mods::HALFTIME) {
        mods.remove(Mods::HALFTIME);
    }
    // Difficulty conflicts: HR beats EZ
    if mods.contains(Mods::HARDROCK) && mods.contains(Mods::EASY) {
        mods.remove(Mods::EASY);
    }
    // Fail conflicts: NF beats SD/PF; PF beats SD
    if mods.contains(Mods::NOFAIL) {
        mods.remove(Mods::SUDDENDEATH | Mods::PERFECT);
    } else if mods.contains(Mods::PERFECT) {
        mods.remove(Mods::SUDDENDEATH);
    }
    // Assist conflicts: RX beats AP
    if mods.contains(Mods::RELAX) && mods.contains(Mods::AUTOPILOT) {
        mods.remove(Mods::AUTOPILOT);
    }

    mods
}

/// Strips room-level-only speed mods (`DT | NC | HT`) and resolves invalid combinations for a FreeMod slot.
pub fn sanitize_freemod_bits(mods: u32) -> u32 {
    let mut m = Mods::from_bits_truncate(mods);
    m.remove(Mods::SPEED_CHANGING | Mods::SCOREV2);
    resolve_mod_conflicts(m).bits()
}

/// Computes the effective mod bitmask for a slot given room mods, slot mods, and whether FreeMod is active.
pub fn combine_effective_mods(room_mods: u32, slot_mods: u32, freemods: bool) -> u32 {
    let combined = if freemods {
        let room_speed = Mods::from_bits_truncate(room_mods) & (Mods::SPEED_CHANGING | Mods::SCOREV2);
        let slot = Mods::from_bits_truncate(sanitize_freemod_bits(slot_mods));
        room_speed | slot
    } else {
        Mods::from_bits_truncate(room_mods)
    };
    resolve_mod_conflicts(combined).bits()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiplayer::SYNTHETIC_BOT_ID_BASE;

    #[test]
    fn test_presets_ordered_by_skill() {
        let names = ["6digit", "5digit", "4digit", "3digit", "2digit", "1digit"];
        let mut prev_sr = 0.0;
        let mut prev_acc = 0.0;
        for (i, name) in names.iter().enumerate() {
            let bot = BotProfile::from_preset(name, SYNTHETIC_BOT_ID_BASE + 1 + i as i32).unwrap();
            let BotSource::Procedural(skill) = &bot.source else {
                panic!("expected procedural skill");
            };
            assert!(skill.target_sr > prev_sr, "{} should have higher SR", name);
            assert!(skill.accuracy_mean > prev_acc, "{} should have higher acc", name);
            prev_sr = skill.target_sr;
            prev_acc = skill.accuracy_mean;
        }
        assert!(BotProfile::from_preset("unknown", SYNTHETIC_BOT_ID_BASE + 99).is_none());
    }

    #[test]
    fn test_rival_scaling() {
        let mut low = Player::new("Low".to_string());
        low.pp = 1_000;
        low.acc = 95.0;
        low.rank = 250_000;

        let mut high = Player::new("High".to_string());
        high.pp = 14_000;
        high.acc = 99.1;
        high.rank = 300;

        let r_low = BotProfile::rival_for(&low, SYNTHETIC_BOT_ID_BASE + 1);
        let r_high = BotProfile::rival_for(&high, SYNTHETIC_BOT_ID_BASE + 2);

        let BotSource::Procedural(s_low) = r_low.source else { panic!() };
        let BotSource::Procedural(s_high) = r_high.source else { panic!() };

        assert!(s_high.target_sr > s_low.target_sr + 3.0);
        assert!((s_low.accuracy_mean - 95.0).abs() < 1e-6);
        assert!((s_high.accuracy_mean - 99.1).abs() < 1e-6);
        assert_eq!(r_high.rank, 300);
    }

    #[test]
    fn test_mod_sanitization_and_combination() {
        // FreeMod strips DT/NC/HT and resolves HR vs EZ and NF vs SD/PF
        let dirty = (Mods::HIDDEN | Mods::DOUBLETIME | Mods::HALFTIME | Mods::HARDROCK | Mods::EASY | Mods::NOFAIL | Mods::PERFECT).bits();
        let clean = Mods::from_bits_truncate(sanitize_freemod_bits(dirty));
        assert!(!clean.intersects(Mods::SPEED_CHANGING));
        assert!(clean.contains(Mods::HIDDEN));
        assert!(clean.contains(Mods::HARDROCK));
        assert!(!clean.contains(Mods::EASY));
        assert!(clean.contains(Mods::NOFAIL));
        assert!(!clean.contains(Mods::PERFECT));

        // Combine in FreeMod room keeps room DT + slot HDHR
        let room = (Mods::DOUBLETIME | Mods::EASY).bits();
        let slot = (Mods::HIDDEN | Mods::HARDROCK | Mods::HALFTIME).bits();
        let eff = Mods::from_bits_truncate(combine_effective_mods(room, slot, true));
        assert!(eff.contains(Mods::DOUBLETIME));
        assert!(eff.contains(Mods::HIDDEN | Mods::HARDROCK));
        assert!(!eff.contains(Mods::HALFTIME));
        assert!(!eff.contains(Mods::EASY));

        // Non-FreeMod room ignores slot mods and resolves room conflicts
        let eff_non_fm = Mods::from_bits_truncate(combine_effective_mods((Mods::DOUBLETIME | Mods::HALFTIME).bits(), Mods::HIDDEN.bits(), false));
        assert!(eff_non_fm.contains(Mods::DOUBLETIME));
        assert!(!eff_non_fm.contains(Mods::HALFTIME));
        assert!(!eff_non_fm.contains(Mods::HIDDEN));
    }

    #[test]
    fn test_to_bancho_player() {
        let bot = BotProfile::from_preset("4digit", SYNTHETIC_BOT_ID_BASE + 5).unwrap();
        let p = bot.to_bancho_player(1);
        assert_eq!(p.userid, SYNTHETIC_BOT_ID_BASE + 5);
        assert_eq!(p.name, "Bot [4-digit]");
        assert_eq!(p.mode, 1);
        assert_eq!(p.rank, 4_500);
        assert_eq!(p.pp, 8_200);
    }
}
