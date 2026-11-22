use serde::{Deserialize, Serialize};

use super::channels::{Inlet, ReservoirChannel};
use super::conditions::ReservoirConditions;
use crate::domain::pump::Supervision;

pub const BOOSTER_LEAD_MS: u64 = 2_000;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Demand {
    pub inlet: Option<Inlet>,
    pub mixer: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelDecision {
    pub on: bool,
    pub blocked_by: Option<ReservoirBlocker>,
}

impl ChannelDecision {
    pub const OFF: Self = Self {
        on: false,
        blocked_by: None,
    };
    pub const ON: Self = Self {
        on: true,
        blocked_by: None,
    };

    pub const fn blocked(blocker: ReservoirBlocker) -> Self {
        Self {
            on: false,
            blocked_by: Some(blocker),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DecisionInput {
    pub conditions: Option<ReservoirConditions>,
    pub supervision: Supervision,
    pub fault: bool,
    pub outputs_uncertain: bool,
    pub water_change_active: bool,
    pub resting: [bool; 2],
    pub demand: Demand,
    pub ro_valve_open_ms: u64,
}

pub fn admit(input: &DecisionInput) -> Result<ReservoirConditions, ReservoirBlocker> {
    if input.fault {
        return Err(ReservoirBlocker::Fault);
    }
    if input
        .conditions
        .is_some_and(|conditions| conditions.leaking())
    {
        return Err(ReservoirBlocker::Leak);
    }
    if input.supervision != Supervision::Active {
        return Err(ReservoirBlocker::SupervisionUnavailable);
    }
    input.conditions.ok_or(ReservoirBlocker::LevelsUnavailable)
}

pub fn global_blocker(input: &DecisionInput) -> Option<ReservoirBlocker> {
    admit(input).err()
}

pub fn inlet_blocker(
    input: &DecisionInput,
    conditions: ReservoirConditions,
    inlet: Inlet,
) -> Option<ReservoirBlocker> {
    if input.outputs_uncertain {
        Some(ReservoirBlocker::Interlock)
    } else if conditions.full {
        Some(ReservoirBlocker::ReservoirFull)
    } else if input.water_change_active {
        Some(ReservoirBlocker::WaterChange)
    } else if input.resting[inlet.index()] {
        Some(ReservoirBlocker::DutyRest)
    } else {
        None
    }
}

pub fn decide(input: DecisionInput) -> [ChannelDecision; 4] {
    let conditions = match admit(&input) {
        Ok(conditions) => conditions,
        Err(blocker) => return [ChannelDecision::blocked(blocker); 4],
    };

    let mut decisions = [ChannelDecision::OFF; 4];
    for inlet in Inlet::ALL {
        decisions[inlet.valve().index()] = match inlet_blocker(&input, conditions, inlet) {
            Some(blocker) => ChannelDecision::blocked(blocker),
            None if input.demand.inlet == Some(inlet) => ChannelDecision::ON,
            None => ChannelDecision::OFF,
        };
    }

    let ro_valve = decisions[ReservoirChannel::RoValve.index()];
    decisions[ReservoirChannel::Booster.index()] = ChannelDecision {
        on: ro_valve.on && input.ro_valve_open_ms >= BOOSTER_LEAD_MS,
        blocked_by: ro_valve.blocked_by,
    };

    decisions[ReservoirChannel::Mixer.index()] = if conditions.empty {
        ChannelDecision::blocked(ReservoirBlocker::ReservoirEmpty)
    } else if input.demand.mixer {
        ChannelDecision::ON
    } else {
        ChannelDecision::OFF
    };

    decisions
}

#[cfg(test)]
mod tests;
