use serde::{Deserialize, Serialize};

use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

pub const STARVED_LOCKOUTS: usize = 3;
pub const STARVED_WINDOW_S: u64 = 600;
pub const STARVED_CLEAR_S: u64 = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelConditions {
    pub sump_low: bool,
    pub reservoir_empty: bool,
    pub reservoir_ready: bool,
    pub tank_overflow: bool,
    pub sump_full: bool,
    pub sump_high: bool,
}

impl LevelConditions {
    pub fn from_snapshot(
        snapshot: &WaterLevelSnapshot,
        refill_stop: Option<WaterLevelRole>,
    ) -> Self {
        let snapshot = snapshot.clone().with_plausibility();
        let dangerous = |role: WaterLevelRole, danger: WaterLevelState| {
            let reading = snapshot.get(role);
            reading.faulty || reading.state == danger
        };

        Self {
            sump_low: dangerous(WaterLevelRole::SumpPump, WaterLevelState::Dry),
            reservoir_empty: dangerous(WaterLevelRole::SumpReserve, WaterLevelState::Dry),
            reservoir_ready: !dangerous(WaterLevelRole::ReservoirReady, WaterLevelState::Dry),
            tank_overflow: dangerous(WaterLevelRole::MainTank, WaterLevelState::Wet),
            sump_full: refill_stop.is_some_and(|role| dangerous(role, WaterLevelState::Wet)),
            sump_high: dangerous(WaterLevelRole::SumpMechanical, WaterLevelState::Wet)
                || dangerous(WaterLevelRole::SumpBiological, WaterLevelState::Wet),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum Blocker {
    SumpLow,
    ReservoirEmpty,
    TankOverflow,
    LevelsUnavailable,
    ReturnOff,
    SumpFull,
    RecentWaterChange,
    ReservoirLow,
    WaterChange,
    SumpHigh,
    Interlock,
    LevelsUnavailableAfterDrain,
    SupervisionUnavailable,
    StorageUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum Supervision {
    Active,
    Unavailable,
    #[default]
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Draining,
    Settling,
    Refilling,
}

impl Phase {
    pub const fn index(self) -> usize {
        match self {
            Self::Draining => 0,
            Self::Settling => 1,
            Self::Refilling => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelDecision {
    pub on: bool,
    pub blocked_by: Option<Blocker>,
}

impl ChannelDecision {
    const fn running(on: bool) -> Self {
        Self {
            on,
            blocked_by: None,
        }
    }

    const fn blocked(blocker: Blocker) -> Self {
        Self {
            on: false,
            blocked_by: Some(blocker),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DecisionInput {
    pub levels: Option<LevelConditions>,
    pub return_locked_out: bool,
    pub return_forced_off: bool,
    pub uv_forced_off: bool,
    pub phase: Option<Phase>,
    pub return_held_after_drain: bool,
    pub supervision_unavailable: bool,
    pub drain_unrecorded: bool,
}

pub fn decide(input: DecisionInput) -> [ChannelDecision; 4] {
    let return_pump = match input.levels {
        _ if input.return_held_after_drain => {
            ChannelDecision::blocked(Blocker::LevelsUnavailableAfterDrain)
        }
        Some(_) if matches!(input.phase, Some(Phase::Draining | Phase::Settling)) => {
            ChannelDecision::blocked(Blocker::WaterChange)
        }
        Some(levels) if levels.sump_low => ChannelDecision::blocked(Blocker::SumpLow),
        Some(_) if input.return_locked_out => ChannelDecision::blocked(Blocker::SumpLow),
        _ => ChannelDecision::running(!input.return_forced_off),
    };

    let uv = if !return_pump.on {
        ChannelDecision::blocked(Blocker::ReturnOff)
    } else {
        ChannelDecision::running(!input.uv_forced_off)
    };

    let makeup = match input.levels {
        None => ChannelDecision::blocked(Blocker::LevelsUnavailable),
        Some(_) if input.supervision_unavailable => {
            ChannelDecision::blocked(Blocker::SupervisionUnavailable)
        }
        Some(levels) if levels.reservoir_empty => ChannelDecision::blocked(Blocker::ReservoirEmpty),
        Some(levels) if levels.tank_overflow => ChannelDecision::blocked(Blocker::TankOverflow),
        Some(levels) if levels.sump_full => ChannelDecision::blocked(Blocker::SumpFull),
        Some(levels) if levels.sump_high => ChannelDecision::blocked(Blocker::SumpHigh),
        Some(_) if input.return_forced_off => ChannelDecision::blocked(Blocker::ReturnOff),
        Some(_) => ChannelDecision::running(input.phase == Some(Phase::Refilling)),
    };

    let drain = match input.levels {
        None => ChannelDecision::blocked(Blocker::LevelsUnavailable),
        Some(_) if input.phase == Some(Phase::Draining) && input.drain_unrecorded => {
            ChannelDecision::blocked(Blocker::StorageUnavailable)
        }
        Some(_) => ChannelDecision::running(input.phase == Some(Phase::Draining)),
    };

    [return_pump, uv, makeup, drain]
}

#[cfg(test)]
mod levels_tests;
#[cfg(test)]
mod tests;
