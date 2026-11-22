use serde::{Deserialize, Serialize};

use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirConditions {
    pub empty: bool,
    pub ready: bool,
    pub full: bool,
    pub full_faulty: bool,
    pub leak: Option<bool>,
}

impl ReservoirConditions {
    pub fn from_snapshot(snapshot: &WaterLevelSnapshot, leak: Option<bool>) -> Self {
        let reserve = snapshot.get(WaterLevelRole::SumpReserve);
        let ready = snapshot.get(WaterLevelRole::ReservoirReady);
        let full = snapshot.get(WaterLevelRole::ReservoirFull);

        Self {
            empty: reserve.state == WaterLevelState::Dry || reserve.faulty,
            ready: ready.state == WaterLevelState::Wet && !ready.faulty,
            full: full.state == WaterLevelState::Wet || full.faulty,
            full_faulty: full.faulty,
            leak,
        }
    }

    pub const fn leaking(&self) -> bool {
        matches!(self.leak, Some(true))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ReservoirBlocker {
    SupervisionUnavailable,
    LevelsUnavailable,
    Leak,
    Fault,
    Interlock,
    ReservoirFull,
    WaterChange,
    DutyRest,
    ReservoirEmpty,
    TdsUnavailable,
}

#[cfg(test)]
mod tests;
