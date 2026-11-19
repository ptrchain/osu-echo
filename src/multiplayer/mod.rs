pub mod bot;
pub mod commands;
pub mod match_state;
pub mod mimic;
pub mod simulator;

pub const SYNTHETIC_BOT_ID_BASE: i32 = 1_000_000_000;

pub fn is_synthetic_bot_id(id: i32) -> bool {
    id >= SYNTHETIC_BOT_ID_BASE
}
