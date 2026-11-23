use serde::{Deserialize, Serialize};

use super::channels::Inlet;
use super::conditions::ReservoirConditions;
use super::decision::Demand;
use super::outcome::{FillRecord, FillResult, FillTrigger};

pub const EMPTY_AGAIN_MS: u64 = 10_000;

pub const TDS_WAIT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillPhase {
    Idle,
    Priming,
    Mixing,
    Measuring,
    Dosing,
}

impl FillPhase {
    pub const fn code(self) -> u8 {
        match self {
            Self::Idle => 0,
            Self::Priming => 1,
            Self::Mixing => 2,
            Self::Measuring => 3,
            Self::Dosing => 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FillPlan {
    pub(super) target_tds_ppm: Option<u16>,
    pub(super) tolerance_ppm: u16,
    pub(super) tap_pulse_ms: u64,
    pub(super) ro_pulse_ms: u64,
    pub(super) mix_ms: u64,
    pub(super) max_duration_ms: u64,
    pub(super) overtime_ms: u64,
}

impl FillPlan {
    pub(super) const fn pulse_ms(&self, inlet: Inlet) -> u64 {
        match inlet {
            Inlet::Ro => self.ro_pulse_ms,
            Inlet::Tap => self.tap_pulse_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FillStage {
    Priming,
    Mixing,
    Measuring,
    Dosing(Inlet),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillInput {
    pub conditions: ReservoirConditions,
    pub elapsed_ms: u64,
    pub delivered: [bool; 2],
    pub mixed: bool,
    pub tds: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillStep {
    Continue,
    PhaseChanged,
    Finished(FillResult),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    trigger: FillTrigger,
    started_at: Option<u64>,
    plan: FillPlan,
    stage: FillStage,
    finishing: bool,
    stage_ms: u64,
    wall_ms: u64,
    inlet_ms: [u64; 2],
    mix_ms: u64,
    pulses: [u32; 2],
    empty_ms: u64,
    last_tds: Option<f32>,
}

const fn seconds(ms: u64) -> u32 {
    let seconds = ms / 1000;
    if seconds > u32::MAX as u64 {
        u32::MAX
    } else {
        seconds as u32
    }
}

impl Fill {
    pub const fn start(trigger: FillTrigger, started_at: Option<u64>, plan: FillPlan) -> Self {
        Self {
            trigger,
            started_at,
            plan,
            stage: FillStage::Priming,
            finishing: false,
            stage_ms: 0,
            wall_ms: 0,
            inlet_ms: [0; 2],
            mix_ms: 0,
            pulses: [0; 2],
            empty_ms: 0,
            last_tds: None,
        }
    }

    pub const fn trigger(&self) -> FillTrigger {
        self.trigger
    }

    pub const fn started_at(&self) -> Option<u64> {
        self.started_at
    }

    pub const fn last_tds(&self) -> Option<f32> {
        self.last_tds
    }

    pub const fn pulses(&self) -> [u32; 2] {
        self.pulses
    }

    pub const fn finishing(&self) -> bool {
        self.finishing
    }

    pub const fn elapsed_s(&self) -> u32 {
        seconds(self.wall_ms)
    }

    pub const fn inlet_s(&self, inlet: Inlet) -> u32 {
        seconds(self.inlet_ms[inlet.index()])
    }

    pub const fn phase(&self) -> FillPhase {
        match self.stage {
            FillStage::Priming => FillPhase::Priming,
            FillStage::Mixing => FillPhase::Mixing,
            FillStage::Measuring => FillPhase::Measuring,
            FillStage::Dosing(_) => FillPhase::Dosing,
        }
    }

    pub const fn dosing(&self) -> Option<Inlet> {
        match self.stage {
            FillStage::Dosing(inlet) => Some(inlet),
            _ => None,
        }
    }

    pub const fn demand(&self) -> Demand {
        match self.stage {
            FillStage::Priming => Demand {
                inlet: Some(Inlet::Ro),
                mixer: false,
            },
            FillStage::Mixing | FillStage::Measuring => Demand {
                inlet: None,
                mixer: true,
            },
            FillStage::Dosing(inlet) => Demand {
                inlet: Some(inlet),
                mixer: true,
            },
        }
    }

    pub fn advance(&mut self, input: FillInput) -> FillStep {
        let conditions = input.conditions;
        let elapsed = input.elapsed_ms;
        self.wall_ms += elapsed;
        for inlet in Inlet::ALL {
            if input.delivered[inlet.index()] {
                self.inlet_ms[inlet.index()] += elapsed;
            }
        }
        if input.mixed {
            self.mix_ms += elapsed;
        }

        if conditions.leaking() {
            return FillStep::Finished(FillResult::Leak);
        }
        if conditions.full_faulty {
            return FillStep::Finished(FillResult::Blocked);
        }
        if !conditions.full && self.inlet_ms.iter().sum::<u64>() > self.plan.overtime_ms {
            return FillStep::Finished(FillResult::Overtime);
        }
        if self.wall_ms > self.plan.max_duration_ms {
            return FillStep::Finished(FillResult::Timeout);
        }
        self.empty_ms = if conditions.empty && self.stage != FillStage::Priming {
            self.empty_ms + elapsed
        } else {
            0
        };
        if self.empty_ms > EMPTY_AGAIN_MS {
            return FillStep::Finished(FillResult::Blocked);
        }

        match self.stage {
            FillStage::Priming => self.prime(conditions),
            FillStage::Mixing => {
                if input.mixed {
                    self.stage_ms += elapsed;
                }
                if self.stage_ms >= self.plan.mix_ms {
                    self.enter(FillStage::Measuring)
                } else {
                    FillStep::Continue
                }
            }
            FillStage::Measuring => {
                self.stage_ms += elapsed;
                match input.tds {
                    Some(tds) => {
                        self.last_tds = Some(tds);
                        self.after_measuring(tds, conditions)
                    }
                    None if self.stage_ms >= TDS_WAIT_MS => {
                        FillStep::Finished(FillResult::TdsUnavailable)
                    }
                    None => FillStep::Continue,
                }
            }
            FillStage::Dosing(inlet) => {
                if input.delivered[inlet.index()] {
                    self.stage_ms += elapsed;
                }
                if conditions.full {
                    self.reach_full()
                } else if self.plan.target_tds_ppm.is_some()
                    && self.stage_ms >= self.plan.pulse_ms(inlet)
                {
                    self.enter(FillStage::Mixing)
                } else {
                    FillStep::Continue
                }
            }
        }
    }

    fn prime(&mut self, conditions: ReservoirConditions) -> FillStep {
        if conditions.full {
            return self.reach_full();
        }
        if conditions.empty {
            return FillStep::Continue;
        }
        match self.plan.target_tds_ppm {
            None => self.enter(FillStage::Dosing(Inlet::Ro)),
            Some(_) => self.enter(FillStage::Mixing),
        }
    }

    fn reach_full(&mut self) -> FillStep {
        if self.plan.target_tds_ppm.is_none() {
            return FillStep::Finished(FillResult::Completed);
        }
        self.finishing = true;
        self.enter(FillStage::Mixing)
    }

    fn after_measuring(&mut self, tds: f32, conditions: ReservoirConditions) -> FillStep {
        let Some(target) = self.plan.target_tds_ppm else {
            return FillStep::Finished(FillResult::Completed);
        };
        let target = f32::from(target);
        let tolerance = f32::from(self.plan.tolerance_ppm);

        if self.finishing || conditions.full {
            return FillStep::Finished(if (tds - target).abs() <= tolerance {
                FillResult::Completed
            } else {
                FillResult::OffTarget
            });
        }

        self.dose(if tds < target { Inlet::Tap } else { Inlet::Ro })
    }

    fn dose(&mut self, inlet: Inlet) -> FillStep {
        self.pulses[inlet.index()] += 1;
        self.enter(FillStage::Dosing(inlet))
    }

    fn enter(&mut self, stage: FillStage) -> FillStep {
        self.stage = stage;
        self.stage_ms = 0;
        FillStep::PhaseChanged
    }

    pub fn record(
        &self,
        result: FillResult,
        finished_at: Option<u64>,
        latest_tds: Option<f32>,
    ) -> FillRecord {
        FillRecord {
            trigger: self.trigger,
            started_at: self.started_at,
            finished_at,
            result,
            final_tds_ppm: self.last_tds.or(latest_tds),
            ro_s: self.inlet_s(Inlet::Ro),
            tap_s: self.inlet_s(Inlet::Tap),
            mix_s: seconds(self.mix_ms),
            ro_pulses: self.pulses[Inlet::Ro.index()],
            tap_pulses: self.pulses[Inlet::Tap.index()],
            duration_s: seconds(self.wall_ms),
        }
    }
}

#[cfg(test)]
mod limits_tests;
#[cfg(test)]
mod tests;
