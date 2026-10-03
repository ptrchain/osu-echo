use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MedalCategory {
    #[serde(rename = "Skill & Dedication")]
    SkillDedication,
    #[serde(rename = "Hush-Hush")]
    HushHush,
    #[serde(rename = "Mod Introduction")]
    ModIntroduction,
    #[serde(rename = "Beatmap Packs")]
    BeatmapPacks,
}

impl MedalCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            MedalCategory::SkillDedication => "Skill & Dedication",
            MedalCategory::HushHush => "Hush-Hush",
            MedalCategory::ModIntroduction => "Mod Introduction",
            MedalCategory::BeatmapPacks => "Beatmap Packs",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MedalDefinition {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub category: MedalCategory,
    pub icon_url: String,
    pub mode: Option<i32>,
    pub ordering: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMedalRecord {
    pub medal_id: i32,
    pub achieved_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserMedalDisplay {
    pub id: i32,
    pub name: String,
    pub description: String,
    pub category: String,
    pub icon_url: String,
    pub achieved_at: i64,
}

static MEDALS_RAW: &str = include_str!("../../assets/medals.json");
static MEDALS_MAP: OnceLock<HashMap<i32, MedalDefinition>> = OnceLock::new();
static MEDALS_LIST: OnceLock<Vec<MedalDefinition>> = OnceLock::new();

fn init_medals() -> (&'static [MedalDefinition], &'static HashMap<i32, MedalDefinition>) {
    let list = MEDALS_LIST.get_or_init(|| {
        serde_json::from_str::<Vec<MedalDefinition>>(MEDALS_RAW).unwrap_or_default()
    });
    let map = MEDALS_MAP.get_or_init(|| {
        list.iter().map(|m| (m.id, m.clone())).collect()
    });
    (list.as_slice(), map)
}

pub fn get_all_medals() -> &'static [MedalDefinition] {
    init_medals().0
}

pub fn get_medal_by_id(id: i32) -> Option<&'static MedalDefinition> {
    init_medals().1.get(&id)
}

pub fn get_medal_by_slug(slug: &str) -> Option<&'static MedalDefinition> {
    get_all_medals().iter().find(|m| m.icon_url == slug)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_medals_catalog_loaded() {
        let all = get_all_medals();
        assert_eq!(all.len(), 352, "Should load all 352 official osu! medals");

        // Verify key skill passes and FCs
        let m55 = get_medal_by_id(55).expect("1★ Pass medal 55 should exist");
        assert_eq!(m55.name, "Rising Star");
        assert_eq!(m55.icon_url, "osu-skill-pass-1");
        assert_eq!(m55.category, MedalCategory::SkillDedication);

        let m63 = get_medal_by_id(63).expect("1★ FC medal 63 should exist");
        assert_eq!(m63.name, "Totality");
        assert_eq!(m63.icon_url, "osu-skill-fc-1");

        // Verify combo medals
        let c500 = get_medal_by_id(1).expect("500 combo medal should exist");
        assert_eq!(c500.name, "500 Combo");
        assert_eq!(c500.icon_url, "osu-combo-500");

        let c2000 = get_medal_by_id(5).expect("2000 combo medal should exist");
        assert_eq!(c2000.name, "2,000 Combo");
        assert_eq!(c2000.icon_url, "osu-combo-2000");

        // Verify mod intro
        let dt = get_medal_by_id(122).expect("DT medal should exist");
        assert_eq!(dt.name, "Time And A Half");
        assert_eq!(dt.category, MedalCategory::ModIntroduction);

        // Verify hush hush
        let jackpot = get_medal_by_id(41).expect("Jackpot medal should exist");
        assert_eq!(jackpot.name, "Jackpot");
        assert_eq!(jackpot.category, MedalCategory::HushHush);
    }
}
