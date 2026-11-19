use crate::multiplayer::bot::{combine_effective_mods, sanitize_freemod_bits, BotProfile};
use crate::multiplayer::SYNTHETIC_BOT_ID_BASE;
use crate::packets::{MatchWire, ScoreFrame, MATCH_SLOT_HAS_PLAYER};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotStatus {
    Open = 1,
    Locked = 2,
    NotReady = 4,
    Ready = 8,
    NoMap = 16,
    Playing = 32,
    Complete = 64,
    Quit = 128,
}

impl SlotStatus {
    pub const HAS_PLAYER_MASK: u8 = MATCH_SLOT_HAS_PLAYER;

    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Open,
            2 => Self::Locked,
            4 => Self::NotReady,
            8 => Self::Ready,
            16 => Self::NoMap,
            32 => Self::Playing,
            64 => Self::Complete,
            128 => Self::Quit,
            _ => Self::Open,
        }
    }

    pub fn has_player(self) -> bool {
        ((self as u8) & Self::HAS_PLAYER_MASK) != 0
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Team {
    Neutral = 0,
    Blue = 1,
    Red = 2,
}

impl Team {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Blue,
            2 => Self::Red,
            _ => Self::Neutral,
        }
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeamType {
    HeadToHead = 0,
    TagCoop = 1,
    TeamVs = 2,
    TagTeamVs = 3,
}

impl TeamType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::TagCoop,
            2 => Self::TeamVs,
            3 => Self::TagTeamVs,
            _ => Self::HeadToHead,
        }
    }

    pub fn is_team_mode(self) -> bool {
        matches!(self, Self::TeamVs | Self::TagTeamVs)
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinCondition {
    Score = 0,
    Accuracy = 1,
    Combo = 2,
    ScoreV2 = 3,
}

impl WinCondition {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Accuracy,
            2 => Self::Combo,
            3 => Self::ScoreV2,
            _ => Self::Score,
        }
    }
}

#[derive(Debug, Clone)]
pub struct BotSlotState {
    pub profile: BotProfile,
    pub timeline: Vec<ScoreFrame>,
    pub last_sent_idx: usize,
    pub has_failed: bool,
    pub fail_time_ms: Option<i32>,
}

impl BotSlotState {
    pub fn new(profile: BotProfile) -> Self {
        Self {
            profile,
            timeline: Vec::new(),
            last_sent_idx: 0,
            has_failed: false,
            fail_time_ms: None,
        }
    }

    pub fn reset_playback(&mut self) {
        self.timeline.clear();
        self.last_sent_idx = 0;
        self.has_failed = false;
        self.fail_time_ms = None;
    }
}

#[derive(Debug, Clone)]
pub struct MatchSlot {
    pub status: SlotStatus,
    pub team: Team,
    pub user_id: i32,
    pub mods: u32,
    pub bot: Option<BotSlotState>,
}

impl MatchSlot {
    pub fn open() -> Self {
        Self {
            status: SlotStatus::Open,
            team: Team::Neutral,
            user_id: 0,
            mods: 0,
            bot: None,
        }
    }

    pub fn is_open(&self) -> bool {
        self.status == SlotStatus::Open && self.user_id == 0 && self.bot.is_none()
    }

    pub fn is_occupied(&self) -> bool {
        self.status.has_player() && self.user_id != 0
    }

    pub fn is_bot(&self) -> bool {
        self.bot.is_some()
    }

    pub fn is_human(&self) -> bool {
        self.is_occupied() && self.bot.is_none()
    }
}

#[derive(Debug, Clone)]
pub struct MatchRoom {
    pub id: u16,
    pub name: String,
    pub password: String,
    pub host_id: i32,
    pub beatmap_id: i32,
    pub beatmap_name: String,
    pub beatmap_md5: String,
    pub mode: u8,
    pub mods: u32,
    pub freemods: bool,
    pub team_type: TeamType,
    pub win_condition: WinCondition,
    pub in_progress: bool,
    pub seed: i32,
    pub rng_seed_override: Option<u64>,
    pub run_nonce: u64,
    pub human_last_frame: Option<ScoreFrame>,
    pub human_failed: bool,
    pub next_bot_id: i32,
    pub slots: [MatchSlot; 16],
}

impl MatchRoom {
    pub fn from_create_wire(id: u16, host_id: i32, wire: &MatchWire) -> Self {
        let team_type = TeamType::from_u8(wire.team_type);
        let mut slots: [MatchSlot; 16] = std::array::from_fn(|i| {
            let mut slot = MatchSlot::open();
            if SlotStatus::from_u8(wire.slot_status[i]) == SlotStatus::Locked {
                slot.status = SlotStatus::Locked;
            }
            slot
        });

        // Place human host in slot 0
        slots[0] = MatchSlot {
            status: SlotStatus::NotReady,
            team: if team_type.is_team_mode() { Team::Blue } else { Team::Neutral },
            user_id: host_id,
            mods: 0,
            bot: None,
        };

        Self {
            id,
            name: if wire.name.is_empty() { "Simulated MP".to_string() } else { wire.name.clone() },
            password: wire.password.clone(),
            host_id,
            beatmap_id: wire.beatmap_id,
            beatmap_name: wire.beatmap_name.clone(),
            beatmap_md5: wire.beatmap_md5.clone(),
            mode: wire.mode,
            mods: wire.mods,
            freemods: wire.freemods != 0,
            team_type,
            win_condition: WinCondition::from_u8(wire.win_condition),
            in_progress: false,
            seed: wire.seed,
            rng_seed_override: None,
            run_nonce: 0,
            human_last_frame: None,
            human_failed: false,
            next_bot_id: SYNTHETIC_BOT_ID_BASE + 1,
            slots,
        }
    }

    pub fn alloc_synthetic_bot_id(&mut self) -> i32 {
        let id = self.next_bot_id;
        self.next_bot_id = self.next_bot_id.saturating_add(1);
        id
    }

    /// Reassigns `profile.userid` to a fresh synthetic ID if it collides with reserved IDs
    /// (local player, BanchoBot, Tillerino, <= 4) or any user already occupying a slot in the room.
    pub fn ensure_unique_bot_userid(&mut self, profile: &mut BotProfile, protected_ids: &[i32]) {
        let collides = profile.userid <= 4
            || protected_ids.contains(&profile.userid)
            || self.slots.iter().any(|s| s.is_occupied() && s.user_id == profile.userid);
        if collides {
            profile.userid = self.alloc_synthetic_bot_id();
        }
    }

    pub fn to_wire(&self) -> MatchWire {
        let mut slot_status = [SlotStatus::Open as u8; 16];
        let mut slot_team = [Team::Neutral as u8; 16];
        let mut slot_mods = [0u32; 16];
        let mut user_ids = Vec::new();

        for (i, slot) in self.slots.iter().enumerate() {
            slot_status[i] = slot.status as u8;
            slot_team[i] = slot.team as u8;
            slot_mods[i] = if self.freemods { sanitize_freemod_bits(slot.mods) } else { 0 };
            if slot.status.has_player() {
                user_ids.push(slot.user_id);
            }
        }

        MatchWire {
            match_id: self.id,
            in_progress: u8::from(self.in_progress),
            match_type: 0,
            mods: self.mods,
            name: self.name.clone(),
            password: self.password.clone(),
            beatmap_name: self.beatmap_name.clone(),
            beatmap_id: self.beatmap_id,
            beatmap_md5: self.beatmap_md5.clone(),
            slot_status,
            slot_team,
            user_ids,
            host_id: self.host_id,
            mode: self.mode,
            win_condition: self.win_condition as u8,
            team_type: self.team_type as u8,
            freemods: u8::from(self.freemods),
            slot_mods,
            seed: self.seed,
        }
    }

    pub fn find_open_slot(&self) -> Option<usize> {
        self.slots.iter().position(|s| s.is_open())
    }

    pub fn find_human_slot(&self, human_id: i32) -> Option<usize> {
        self.slots.iter().position(|s| s.is_human() && s.user_id == human_id)
    }

    fn next_balanced_team(&self) -> Team {
        if !self.team_type.is_team_mode() {
            return Team::Neutral;
        }
        let blue_count = self.slots.iter().filter(|s| s.is_occupied() && s.team == Team::Blue).count();
        let red_count = self.slots.iter().filter(|s| s.is_occupied() && s.team == Team::Red).count();
        if blue_count <= red_count {
            Team::Blue
        } else {
            Team::Red
        }
    }

    pub fn add_bot(&mut self, mut profile: BotProfile) -> Result<usize, &'static str> {
        let slot_idx = self.find_open_slot().ok_or("Room has no open slots")?;
        self.ensure_unique_bot_userid(&mut profile, &[self.host_id]);
        let team = self.next_balanced_team();
        let slot_mods = if self.freemods {
            sanitize_freemod_bits(profile.preferred_mods())
        } else {
            0
        };
        let uid = profile.userid;
        self.slots[slot_idx] = MatchSlot {
            status: SlotStatus::Ready,
            team,
            user_id: uid,
            mods: slot_mods,
            bot: Some(BotSlotState::new(profile)),
        };
        Ok(slot_idx)
    }

    pub fn remove_bot_at(&mut self, slot_idx: usize) -> Option<BotProfile> {
        let slot = self.slots.get_mut(slot_idx)?;
        let bot_state = slot.bot.take()?;
        *slot = MatchSlot::open();
        Some(bot_state.profile)
    }

    pub fn clear_bots(&mut self) -> Vec<BotProfile> {
        let mut removed = Vec::new();
        for i in 0..16 {
            if let Some(profile) = self.remove_bot_at(i) {
                removed.push(profile);
            }
        }
        removed
    }

    pub fn set_all_bots_ready(&mut self) {
        for slot in &mut self.slots {
            if slot.is_bot() {
                slot.status = SlotStatus::Ready;
            }
        }
    }

    pub fn apply_freemod_constraints(&mut self) {
        if self.freemods {
            for slot in &mut self.slots {
                if let Some(ref bot) = slot.bot {
                    let target = if slot.mods != 0 { slot.mods } else { bot.profile.preferred_mods() };
                    slot.mods = sanitize_freemod_bits(target);
                } else {
                    slot.mods = sanitize_freemod_bits(slot.mods);
                }
            }
        } else {
            for slot in &mut self.slots {
                slot.mods = 0;
            }
        }
    }

    pub fn assign_balanced_teams(&mut self) {
        if !self.team_type.is_team_mode() {
            for slot in &mut self.slots {
                slot.team = Team::Neutral;
            }
            return;
        }
        let mut blue_count = self.slots.iter().filter(|s| s.is_occupied() && s.team == Team::Blue).count();
        let mut red_count = self.slots.iter().filter(|s| s.is_occupied() && s.team == Team::Red).count();
        for slot in &mut self.slots {
            if slot.is_occupied() && slot.team == Team::Neutral {
                if blue_count <= red_count {
                    slot.team = Team::Blue;
                    blue_count += 1;
                } else {
                    slot.team = Team::Red;
                    red_count += 1;
                }
            }
        }
    }

    pub fn reset_after_match(&mut self) {
        self.in_progress = false;
        self.human_last_frame = None;
        self.human_failed = false;
        for slot in &mut self.slots {
            if let Some(ref mut bot) = slot.bot {
                bot.reset_playback();
                slot.status = SlotStatus::Ready;
            } else if slot.status == SlotStatus::Playing || slot.status == SlotStatus::Complete {
                slot.status = SlotStatus::NotReady;
            }
        }
    }

    pub fn effective_mods_for_slot(&self, slot_idx: usize) -> u32 {
        let slot_mods = self.slots.get(slot_idx).map(|s| s.mods).unwrap_or(0);
        combine_effective_mods(self.mods, slot_mods, self.freemods)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::multiplayer::bot::BotSource;
    use crate::types::mods::Mods;

    fn make_room(freemods: bool, team_type: TeamType) -> MatchRoom {
        let mut wire = MatchWire::default();
        wire.name = "Test Lobby".to_string();
        wire.freemods = u8::from(freemods);
        wire.team_type = team_type as u8;
        wire.slot_status = [SlotStatus::Open as u8; 16];
        wire.slot_status[15] = SlotStatus::Locked as u8;
        MatchRoom::from_create_wire(1, 2, &wire)
    }

    #[test]
    fn test_room_creation_and_wire_mapping() {
        let room = make_room(false, TeamType::HeadToHead);
        assert_eq!(room.find_human_slot(2), Some(0));
        assert_eq!(room.slots[0].status, SlotStatus::NotReady);
        assert_eq!(room.slots[15].status, SlotStatus::Locked);

        let wire = room.to_wire();
        assert_eq!(wire.match_id, 1);
        assert_eq!(wire.host_id, 2);
        assert_eq!(wire.user_ids, vec![2]);
        assert_eq!(wire.slot_status[0], SlotStatus::NotReady as u8);
        assert_eq!(wire.slot_status[1], SlotStatus::Open as u8);
        assert_eq!(wire.slot_status[15], SlotStatus::Locked as u8);
    }

    #[test]
    fn test_add_remove_clear_bots_and_team_balance() {
        let mut room = make_room(false, TeamType::TeamVs);
        assert_eq!(room.slots[0].team, Team::Blue);

        let id1 = room.alloc_synthetic_bot_id();
        let b1 = BotProfile::from_preset("5digit", id1).unwrap();
        let s1 = room.add_bot(b1).unwrap();
        assert_eq!(s1, 1);
        assert_eq!(room.slots[1].team, Team::Red);
        assert_eq!(room.slots[1].status, SlotStatus::Ready);

        let id2 = room.alloc_synthetic_bot_id();
        let b2 = BotProfile::from_preset("4digit", id2).unwrap();
        let s2 = room.add_bot(b2).unwrap();
        assert_eq!(s2, 2);
        assert_eq!(room.slots[2].team, Team::Blue);

        let wire = room.to_wire();
        assert_eq!(wire.user_ids, vec![2, id1, id2]);

        let removed = room.remove_bot_at(1).unwrap();
        assert_eq!(removed.userid, id1);
        assert!(room.slots[1].is_open());

        // Cannot remove human slot via remove_bot_at
        assert!(room.remove_bot_at(0).is_none());
        assert_eq!(room.slots[0].user_id, 2);

        let cleared = room.clear_bots();
        assert_eq!(cleared.len(), 1);
        assert_eq!(cleared[0].userid, id2);
        assert!(room.slots[2].is_open());
    }

    #[test]
    fn test_freemod_constraints_and_reset_after_match() {
        let mut room = make_room(true, TeamType::HeadToHead);
        room.mods = Mods::DOUBLETIME.bits();

        let id = room.alloc_synthetic_bot_id();
        let mut bot = BotProfile::from_preset("3digit", id).unwrap();
        if let BotSource::Procedural(ref mut skill) = bot.source {
            // Include DT (should be stripped from slot) + HDHR
            skill.preferred_mods = (Mods::DOUBLETIME | Mods::HIDDEN | Mods::HARDROCK).bits();
        }
        let slot_idx = room.add_bot(bot).unwrap();
        assert_eq!(room.slots[slot_idx].mods, (Mods::HIDDEN | Mods::HARDROCK).bits());

        // Effective mods combine room DT + slot HDHR
        let eff = Mods::from_bits_truncate(room.effective_mods_for_slot(slot_idx));
        assert_eq!(eff, Mods::DOUBLETIME | Mods::HIDDEN | Mods::HARDROCK);

        // Simulate in-progress then reset
        room.in_progress = true;
        room.slots[0].status = SlotStatus::Playing;
        room.slots[slot_idx].status = SlotStatus::Playing;
        if let Some(ref mut bs) = room.slots[slot_idx].bot {
            bs.has_failed = true;
            bs.fail_time_ms = Some(12_000);
            bs.last_sent_idx = 5;
        }

        room.reset_after_match();
        assert!(!room.in_progress);
        assert_eq!(room.slots[0].status, SlotStatus::NotReady);
        assert_eq!(room.slots[slot_idx].status, SlotStatus::Ready);
        let bs = room.slots[slot_idx].bot.as_ref().unwrap();
        assert!(!bs.has_failed);
        assert_eq!(bs.fail_time_ms, None);
        assert_eq!(bs.last_sent_idx, 0);
    }

    #[test]
    fn test_colliding_bot_userid_reassigned_to_synthetic() {
        use crate::multiplayer::bot::SkillProfile;
        use crate::multiplayer::is_synthetic_bot_id;

        let mut room = make_room(false, TeamType::HeadToHead);
        // Local human host has userid = 2; adding peppy (userid = 2) must NOT collide!
        let peppy = BotProfile {
            userid: 2,
            name: "peppy".to_string(),
            country: 16,
            rank: 500_000,
            pp: 200,
            acc: 95.0,
            source: BotSource::MimicPlayer {
                username: "peppy".to_string(),
                osu_user_id: Some(2),
                fallback_skill: SkillProfile::default(),
            },
        };
        let slot1 = room.add_bot(peppy).unwrap();
        assert_ne!(room.slots[slot1].user_id, 2);
        assert!(is_synthetic_bot_id(room.slots[slot1].user_id));

        // Adding the same mimicked player twice must give the second copy a distinct synthetic ID
        let mrekk = BotProfile {
            userid: 7562902,
            name: "mrekk".to_string(),
            country: 16,
            rank: 1,
            pp: 30_000,
            acc: 98.5,
            source: BotSource::MimicPlayer {
                username: "mrekk".to_string(),
                osu_user_id: Some(7562902),
                fallback_skill: SkillProfile::default(),
            },
        };
        let s2 = room.add_bot(mrekk.clone()).unwrap();
        let s3 = room.add_bot(mrekk).unwrap();
        assert_eq!(room.slots[s2].user_id, 7562902);
        assert_ne!(room.slots[s3].user_id, 7562902);
        assert!(is_synthetic_bot_id(room.slots[s3].user_id));
    }
}
