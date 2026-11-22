use super::channels::{Inlet, ReservoirChannel};
use super::conditions::{ReservoirBlocker, ReservoirConditions};
use crate::domain::pump::Supervision;

pub const BOOSTER_LEAD_MS: u64 = 2_000;

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
    pub uncertain: bool,
    pub water_change_active: bool,
    pub resting: [bool; 2],
    pub demand: Demand,
    pub ro_valve_open_ms: u64,
}

fn global_blocker(input: &DecisionInput) -> Option<ReservoirBlocker> {
    if input.supervision != Supervision::Active {
        return Some(ReservoirBlocker::SupervisionUnavailable);
    }
    match input.conditions {
        None => Some(ReservoirBlocker::LevelsUnavailable),
        Some(conditions) if conditions.leaking() => Some(ReservoirBlocker::Leak),
        Some(_) if input.fault => Some(ReservoirBlocker::Fault),
        Some(_) => None,
    }
}

fn inlet_blocker(
    input: &DecisionInput,
    conditions: ReservoirConditions,
    inlet: Inlet,
) -> Option<ReservoirBlocker> {
    if input.uncertain {
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

pub fn decide(input: &DecisionInput) -> [ChannelDecision; 4] {
    if let Some(blocker) = global_blocker(input) {
        return [ChannelDecision::blocked(blocker); 4];
    }
    let Some(conditions) = input.conditions else {
        return [ChannelDecision::blocked(ReservoirBlocker::LevelsUnavailable); 4];
    };

    let mut decisions = [ChannelDecision::OFF; 4];
    if let Some(inlet) = input.demand.inlet {
        let blocker = inlet_blocker(input, conditions, inlet);
        match (inlet, blocker) {
            (Inlet::Ro, Some(blocker)) => {
                decisions[ReservoirChannel::RoValve.index()] = ChannelDecision::blocked(blocker);
                decisions[ReservoirChannel::Booster.index()] = ChannelDecision::blocked(blocker);
            }
            (Inlet::Ro, None) => {
                decisions[ReservoirChannel::RoValve.index()] = ChannelDecision::ON;
                if input.ro_valve_open_ms >= BOOSTER_LEAD_MS {
                    decisions[ReservoirChannel::Booster.index()] = ChannelDecision::ON;
                }
            }
            (Inlet::Tap, Some(blocker)) => {
                decisions[ReservoirChannel::TapValve.index()] = ChannelDecision::blocked(blocker);
            }
            (Inlet::Tap, None) => {
                decisions[ReservoirChannel::TapValve.index()] = ChannelDecision::ON;
            }
        }
    }

    if input.demand.mixer {
        decisions[ReservoirChannel::Mixer.index()] = if conditions.empty {
            ChannelDecision::blocked(ReservoirBlocker::ReservoirEmpty)
        } else {
            ChannelDecision::ON
        };
    }

    decisions
}

#[cfg(test)]
mod tests;
