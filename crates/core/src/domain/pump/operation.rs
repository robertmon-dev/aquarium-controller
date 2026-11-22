#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum OperationKind {
    TopOff,
    WaterChange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationPlan {
    pub kind: OperationKind,
    pub drain_s: u32,
    pub settle_s: u32,
    pub refill_s: u32,
}

impl OperationPlan {
    pub const fn top_off(duration_s: u32) -> Self {
        Self {
            kind: OperationKind::TopOff,
            drain_s: 0,
            settle_s: 0,
            refill_s: duration_s,
        }
    }

    pub const fn water_change(plan: WaterChangePlan) -> Self {
        Self {
            kind: OperationKind::WaterChange,
            drain_s: plan.drain_s,
            settle_s: plan.settle_s,
            refill_s: plan.refill_s,
        }
    }

    const fn target_s(&self, phase: Phase) -> u32 {
        match phase {
            Phase::Draining => self.drain_s,
            Phase::Settling => self.settle_s,
            Phase::Refilling => self.refill_s,
        }
    }

    const fn first_phase(&self) -> Phase {
        match self.kind {
            OperationKind::TopOff => Phase::Refilling,
            OperationKind::WaterChange => Phase::Draining,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum OperationResult {
    Completed,
    Overflow,
    SumpFull,
    ReservoirEmpty,
    Aborted,
    InterruptedByReboot,
    RefillTimeout,
    Incomplete,
    DrainTimeout,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct OperationRecord {
    pub kind: OperationKind,

    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub started_at: Option<u64>,

    pub drain_ran_s: u32,
    pub settle_ran_s: u32,
    pub refill_ran_s: u32,
    pub result: OperationResult,
    pub resumed: bool,

    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable, as = "Option<f64>"))]
    pub finished_at: Option<u64>,
}

impl OperationRecord {
    pub fn graded(mut self, flows: FlowRates, sump_low: bool) -> Self {
        if self.kind != OperationKind::WaterChange || self.result != OperationResult::Completed {
            return self;
        }

        let short = match (flows.drain, flows.makeup) {
            (Some(drain), Some(makeup)) => {
                u64::from(self.refill_ran_s) * u64::from(makeup) * 100
                    < MIN_REFILL_SHARE_PCT * u64::from(self.drain_ran_s) * u64::from(drain)
            }
            _ => false,
        };

        if short || sump_low {
            self.result = OperationResult::Incomplete;
        }

        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    PhaseChanged,
    Finished(OperationResult),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub plan: OperationPlan,
    pub phase: Phase,
    pub ran_ms: [u64; 3],
    pub started_at: Option<u64>,
    pub resumed: bool,

    #[serde(default)]
    pub refill_wall_ms: u64,

    #[serde(default)]
    pub drain_switched: bool,

    #[serde(default)]
    pub drain_wall_ms: u64,

    #[serde(skip)]
    pub refill_armed: bool,
}

pub const fn refill_timeout_s(refill_s: u32) -> u32 {
    phase_timeout_s(refill_s)
}

pub const fn phase_timeout_s(target_s: u32) -> u32 {
    let doubled = target_s.saturating_mul(2);
    let graced = target_s.saturating_add(REFILL_TIMEOUT_GRACE_S);

    if doubled > graced {
        doubled
    } else {
        graced
    }
}

impl Operation {
    pub const fn start(plan: OperationPlan, started_at: Option<u64>) -> Self {
        Self {
            phase: plan.first_phase(),
            plan,
            ran_ms: [0; 3],
            started_at,
            resumed: false,
            refill_wall_ms: 0,
            drain_switched: false,
            drain_wall_ms: 0,
            refill_armed: false,
        }
    }

    pub fn drained(&self) -> bool {
        self.drain_switched || self.ran_ms[Phase::Draining.index()] > 0
    }

    pub const fn ran_s(&self, phase: Phase) -> u32 {
        (self.ran_ms[phase.index()] / 1000) as u32
    }

    pub fn remaining_s(&self) -> u32 {
        let target_ms = u64::from(self.plan.target_s(self.phase)) * 1000;
        target_ms
            .saturating_sub(self.ran_ms[self.phase.index()])
            .div_ceil(1000) as u32
    }

    pub fn advance(&mut self, ran_ms: u64, wall_ms: u64, levels: Option<LevelConditions>) -> Step {
        let Some(levels) = levels else {
            return Step::Finished(OperationResult::Aborted);
        };

        let slot = self.phase.index();
        self.ran_ms[slot] += ran_ms;
        let done = self.ran_ms[slot] >= u64::from(self.plan.target_s(self.phase)) * 1000;

        match self.phase {
            Phase::Draining => self.drain_wall_ms += wall_ms,
            Phase::Refilling => {
                self.refill_wall_ms += wall_ms;
                self.refill_armed |= ran_ms > 0;
            }
            Phase::Settling => {}
        }

        let timed_out =
            self.refill_wall_ms > u64::from(refill_timeout_s(self.plan.refill_s)) * 1000;
        let drain_timed_out =
            self.drain_wall_ms > u64::from(phase_timeout_s(self.plan.drain_s)) * 1000;

        match self.phase {
            Phase::Draining if done => {
                self.phase = Phase::Settling;
                Step::PhaseChanged
            }
            Phase::Draining if drain_timed_out => Step::Finished(OperationResult::DrainTimeout),
            Phase::Settling if done => {
                self.phase = Phase::Refilling;
                self.refill_wall_ms = 0;
                self.refill_armed = false;
                Step::PhaseChanged
            }
            Phase::Refilling if levels.reservoir_empty => {
                Step::Finished(OperationResult::ReservoirEmpty)
            }
            Phase::Refilling if levels.tank_overflow => Step::Finished(OperationResult::Overflow),
            Phase::Refilling
                if levels.sump_full
                    && (self.plan.kind == OperationKind::TopOff || self.refill_armed) =>
            {
                Step::Finished(OperationResult::SumpFull)
            }
            Phase::Refilling if levels.sump_high => Step::Finished(OperationResult::Overflow),
            Phase::Refilling if done => Step::Finished(OperationResult::Completed),
            Phase::Refilling if timed_out => Step::Finished(OperationResult::RefillTimeout),
            _ => Step::Continue,
        }
    }

    pub fn resume(&mut self) {
        if self.phase == Phase::Settling {
            self.ran_ms[Phase::Settling.index()] = 0;
        }

        self.resumed = true;
        self.refill_armed = false;
    }

    pub fn ready_to_resume(&self, levels: Option<LevelConditions>) -> Result<(), Blocker> {
        let levels = levels.ok_or(Blocker::LevelsUnavailable)?;

        if self.phase == Phase::Refilling && levels.reservoir_empty {
            return Err(Blocker::ReservoirEmpty);
        }

        if self.phase != Phase::Settling {
            if levels.tank_overflow {
                return Err(Blocker::TankOverflow);
            }
            if levels.sump_high {
                return Err(Blocker::SumpHigh);
            }
        }

        Ok(())
    }

    pub const fn record(&self, result: OperationResult) -> OperationRecord {
        OperationRecord {
            kind: self.plan.kind,
            started_at: self.started_at,
            drain_ran_s: self.ran_s(Phase::Draining),
            settle_ran_s: self.ran_s(Phase::Settling),
            refill_ran_s: self.ran_s(Phase::Refilling),
            result,
            resumed: self.resumed,
            finished_at: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub operation: Operation,
    pub checkpoint_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResumeVerdict {
    Resume,
    Wait,
    Suspend,
}

pub fn resume_verdict(
    checkpoint_at: Option<u64>,
    now_epoch: Option<u64>,
    uptime_s: u64,
    window_s: u32,
    ready: bool,
) -> ResumeVerdict {
    let window = u64::from(window_s);
    let waited_too_long = uptime_s >= window;

    let Some(saved) = checkpoint_at.filter(|_| window > 0) else {
        return ResumeVerdict::Suspend;
    };

    let Some(now) = now_epoch else {
        return if waited_too_long {
            ResumeVerdict::Suspend
        } else {
            ResumeVerdict::Wait
        };
    };

    if now < saved || now - saved >= window {
        return ResumeVerdict::Suspend;
    }

    match (ready, waited_too_long) {
        (true, _) => ResumeVerdict::Resume,
        (false, true) => ResumeVerdict::Suspend,
        (false, false) => ResumeVerdict::Wait,
    }
}

pub fn litres(ran_s: u32, flow_lph: Option<u32>) -> Option<f32> {
    flow_lph.map(|flow| ran_s as f32 * flow as f32 / 3600.0)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OperationEvent {
    pub record: OperationRecord,
    pub finished_at: Option<u64>,
    pub drained_l: Option<f32>,
    pub refilled_l: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct StoredOverride {
    #[serde(deserialize_with = "Option::deserialize")]
    pub expires_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RuntimeCounters {
    pub runtime_s: [u64; 4],
    pub reset_at: [Option<u64>; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LastOperations {
    pub top_off: Option<OperationRecord>,
    pub water_change: Option<OperationRecord>,

    #[serde(default)]
    pub drained_at: Option<u64>,
}

impl LastOperations {
    pub fn remember(&mut self, operation: &Operation, record: OperationRecord) {
        match record.kind {
            OperationKind::TopOff => self.top_off = Some(record),
            OperationKind::WaterChange => {
                if operation.drained() {
                    self.drained_at = record.started_at;
                }
                self.water_change = Some(record);
            }
        }
    }

    pub fn last_drain_started_at(&self) -> Option<u64> {
        self.drained_at.or_else(|| {
            self.water_change
                .filter(|record| record.drain_ran_s > 0)
                .and_then(|record| record.started_at)
        })
    }
}
