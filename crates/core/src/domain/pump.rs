use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

use crate::domain::cron::CronExpr;

pub const MIN_RETURN_OVERRIDE_S: u32 = 60;
pub const MAX_RETURN_OVERRIDE_S: u32 = 7200;
pub const MIN_UV_OVERRIDE_S: u32 = 60;
pub const MAX_UV_OVERRIDE_S: u32 = 2_592_000;
pub const MAX_TOP_OFF_S: u32 = 600;
pub const MAX_DRAIN_S: u32 = 900;
pub const MAX_SETTLE_S: u32 = 300;
pub const DEFAULT_SETTLE_S: u32 = 30;
pub const MAX_REFILL_S: u32 = 900;
pub const MAX_WINDOWS: usize = 4;
pub const MAX_CRON_LEN: usize = 64;
pub const MAX_FLOW_LPH: u32 = 10_000;
pub const MAX_RESUME_WINDOW_S: u32 = 3600;
pub const DEFAULT_RESUME_WINDOW_S: u32 = 600;
pub const MAX_WATER_CHANGE_GAP_H: u32 = 168;
pub const DEFAULT_WATER_CHANGE_GAP_H: u32 = 12;
pub const MAX_RESERVOIR_L: u32 = 10_000;
pub const REFILL_TIMEOUT_GRACE_S: u32 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum PumpId {
    Return,
    Uv,
    Makeup,
    Drain,
}

impl PumpId {
    pub const ALL: [Self; 4] = [Self::Return, Self::Uv, Self::Makeup, Self::Drain];

    pub const fn index(self) -> usize {
        match self {
            Self::Return => 0,
            Self::Uv => 1,
            Self::Makeup => 2,
            Self::Drain => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Return => "return",
            Self::Uv => "uv",
            Self::Makeup => "makeup",
            Self::Drain => "drain",
        }
    }

    pub const fn fail_safe_on(self) -> bool {
        matches!(self, Self::Return | Self::Uv)
    }

    pub const fn override_slot(self) -> Option<usize> {
        match self {
            Self::Return => Some(0),
            Self::Uv => Some(1),
            Self::Makeup | Self::Drain => None,
        }
    }
}

impl FromStr for PumpId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|id| id.name() == value)
            .ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LevelConditions {
    pub sump_low: bool,
    pub reservoir_empty: bool,
    pub reservoir_ready: bool,
    pub tank_overflow: bool,
    pub sump_full: bool,
}

impl LevelConditions {
    pub fn from_snapshot(
        snapshot: &WaterLevelSnapshot,
        refill_stop: Option<WaterLevelRole>,
    ) -> Self {
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
}

pub fn decide(input: DecisionInput) -> [ChannelDecision; 4] {
    let return_pump = match input.levels {
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
        Some(levels) if levels.reservoir_empty => ChannelDecision::blocked(Blocker::ReservoirEmpty),
        Some(levels) if levels.tank_overflow => ChannelDecision::blocked(Blocker::TankOverflow),
        Some(levels) if levels.sump_full => ChannelDecision::blocked(Blocker::SumpFull),
        Some(_) if input.return_forced_off => ChannelDecision::blocked(Blocker::ReturnOff),
        Some(_) => ChannelDecision::running(input.phase == Some(Phase::Refilling)),
    };

    let drain = match input.levels {
        None => ChannelDecision::blocked(Blocker::LevelsUnavailable),
        Some(_) => ChannelDecision::running(input.phase == Some(Phase::Draining)),
    };

    [return_pump, uv, makeup, drain]
}

const fn default_settle_s() -> u32 {
    DEFAULT_SETTLE_S
}

const fn default_water_change_gap_h() -> u32 {
    DEFAULT_WATER_CHANGE_GAP_H
}

const fn default_refill_stop_sensor() -> Option<WaterLevelRole> {
    Some(WaterLevelRole::SumpReturnLevel)
}

fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WaterChangePlan {
    pub drain_s: u32,

    #[serde(default = "default_settle_s")]
    pub settle_s: u32,

    pub refill_s: u32,
}

impl WaterChangePlan {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=MAX_DRAIN_S).contains(&self.drain_s) {
            return Err("drain_s must be between 1 and 900");
        }

        if self.settle_s > MAX_SETTLE_S {
            return Err("settle_s must not exceed 300");
        }

        if !(1..=MAX_REFILL_S).contains(&self.refill_s) {
            return Err("refill_s must be between 1 and 900");
        }

        Ok(())
    }
}

pub fn validate_top_off(duration_s: u32) -> Result<(), &'static str> {
    if (1..=MAX_TOP_OFF_S).contains(&duration_s) {
        Ok(())
    } else {
        Err("a top-off lasts 1 to 600 s")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TopOffWindow {
    pub cron: String,
    pub duration_s: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WaterChangeWindow {
    pub cron: String,
    pub drain_s: u32,

    #[serde(default = "default_settle_s")]
    pub settle_s: u32,

    pub refill_s: u32,
}

impl WaterChangeWindow {
    pub const fn plan(&self) -> WaterChangePlan {
        WaterChangePlan {
            drain_s: self.drain_s,
            settle_s: self.settle_s,
            refill_s: self.refill_s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FlowRates {
    #[serde(default)]
    pub makeup: Option<u32>,

    #[serde(default)]
    pub drain: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PumpSettings {
    pub top_off: Vec<TopOffWindow>,
    pub water_change: Vec<WaterChangeWindow>,
    pub flow_lph: FlowRates,
    pub resume_window_s: u32,

    #[serde(default = "default_refill_stop_sensor")]
    pub refill_stop_sensor: Option<WaterLevelRole>,

    #[serde(default = "default_water_change_gap_h")]
    pub min_water_change_gap_h: u32,

    #[serde(default)]
    pub reservoir_ready_l: Option<u32>,
}

impl PumpSettings {
    pub const DEFAULT: Self = Self {
        top_off: Vec::new(),
        water_change: Vec::new(),
        flow_lph: FlowRates {
            makeup: None,
            drain: None,
        },
        resume_window_s: DEFAULT_RESUME_WINDOW_S,
        refill_stop_sensor: default_refill_stop_sensor(),
        min_water_change_gap_h: DEFAULT_WATER_CHANGE_GAP_H,
        reservoir_ready_l: None,
    };

    pub fn drain_fits_reservoir(&self, drain_s: u32) -> bool {
        match (self.flow_lph.drain, self.reservoir_ready_l) {
            (Some(flow), Some(reservoir)) => {
                u64::from(drain_s) * u64::from(flow) <= u64::from(reservoir) * 3600
            }
            _ => true,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.top_off.len() > MAX_WINDOWS || self.water_change.len() > MAX_WINDOWS {
            return Err("at most 4 top-off and 4 water-change windows");
        }

        for cron in self
            .top_off
            .iter()
            .map(|window| &window.cron)
            .chain(self.water_change.iter().map(|window| &window.cron))
        {
            if cron.len() > MAX_CRON_LEN {
                return Err("cron must be at most 64 characters");
            }
        }

        for window in &self.top_off {
            CronExpr::parse(&window.cron)?;
            validate_top_off(window.duration_s)?;
        }

        for window in &self.water_change {
            CronExpr::parse(&window.cron)?;
            window.plan().validate()?;
        }

        for flow in [self.flow_lph.makeup, self.flow_lph.drain]
            .into_iter()
            .flatten()
        {
            if !(1..=MAX_FLOW_LPH).contains(&flow) {
                return Err("flow must be between 1 and 10000 L/h");
            }
        }

        if self.resume_window_s > MAX_RESUME_WINDOW_S {
            return Err("resume_window_s must not exceed 3600");
        }

        if !matches!(
            self.refill_stop_sensor,
            None | Some(
                WaterLevelRole::SumpReturnLevel
                    | WaterLevelRole::SumpMechanical
                    | WaterLevelRole::SumpBiological
            )
        ) {
            return Err(
                "refill_stop_sensor must be sump_return_level, sump_mechanical or sump_biological",
            );
        }

        if self.min_water_change_gap_h > MAX_WATER_CHANGE_GAP_H {
            return Err("min_water_change_gap_h must not exceed 168");
        }

        if self
            .reservoir_ready_l
            .is_some_and(|litres| !(1..=MAX_RESERVOIR_L).contains(&litres))
        {
            return Err("reservoir_ready_l must be between 1 and 10000");
        }

        if self
            .water_change
            .iter()
            .any(|window| !self.drain_fits_reservoir(window.drain_s))
        {
            return Err("a water-change window drains more than reservoir_ready_l holds");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PumpSettingsPatch {
    #[serde(default)]
    pub top_off: Option<Vec<TopOffWindow>>,

    #[serde(default)]
    pub water_change: Option<Vec<WaterChangeWindow>>,

    #[serde(default)]
    pub flow_lph: Option<FlowRates>,

    #[serde(default)]
    pub resume_window_s: Option<u32>,

    #[serde(default, deserialize_with = "nullable")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub refill_stop_sensor: Option<Option<WaterLevelRole>>,

    #[serde(default)]
    pub min_water_change_gap_h: Option<u32>,

    #[serde(default, deserialize_with = "nullable")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub reservoir_ready_l: Option<Option<u32>>,
}

impl PumpSettingsPatch {
    pub fn merge_onto(self, base: &PumpSettings) -> PumpSettings {
        PumpSettings {
            top_off: self.top_off.unwrap_or_else(|| base.top_off.clone()),
            water_change: self
                .water_change
                .unwrap_or_else(|| base.water_change.clone()),
            flow_lph: self.flow_lph.unwrap_or(base.flow_lph),
            resume_window_s: self.resume_window_s.unwrap_or(base.resume_window_s),
            refill_stop_sensor: self.refill_stop_sensor.unwrap_or(base.refill_stop_sensor),
            min_water_change_gap_h: self
                .min_water_change_gap_h
                .unwrap_or(base.min_water_change_gap_h),
            reservoir_ready_l: self.reservoir_ready_l.unwrap_or(base.reservoir_ready_l),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideError {
    NotOverridable,
    Invalid(&'static str),
    NotPersisted(&'static str),
}

pub fn validate_override(id: PumpId, duration_s: Option<u32>) -> Result<(), OverrideError> {
    match (id, duration_s) {
        (PumpId::Makeup | PumpId::Drain, _) => Err(OverrideError::NotOverridable),
        (PumpId::Return, None) => Err(OverrideError::Invalid(
            "a return pump override needs duration_s",
        )),
        (PumpId::Return, Some(seconds))
            if !(MIN_RETURN_OVERRIDE_S..=MAX_RETURN_OVERRIDE_S).contains(&seconds) =>
        {
            Err(OverrideError::Invalid(
                "a return pump override lasts 60 to 7200 s",
            ))
        }
        (PumpId::Uv, Some(seconds))
            if !(MIN_UV_OVERRIDE_S..=MAX_UV_OVERRIDE_S).contains(&seconds) =>
        {
            Err(OverrideError::Invalid(
                "a UV override lasts 60 s to 30 days",
            ))
        }
        _ => Ok(()),
    }
}

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
}

pub const fn refill_timeout_s(refill_s: u32) -> u32 {
    let doubled = refill_s.saturating_mul(2);
    let graced = refill_s.saturating_add(REFILL_TIMEOUT_GRACE_S);

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
        }
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

        if self.phase == Phase::Refilling {
            self.refill_wall_ms += wall_ms;
        }

        let timed_out =
            self.refill_wall_ms > u64::from(refill_timeout_s(self.plan.refill_s)) * 1000;

        match self.phase {
            Phase::Draining if done => {
                self.phase = Phase::Settling;
                Step::PhaseChanged
            }
            Phase::Settling if done => {
                self.phase = Phase::Refilling;
                self.refill_wall_ms = 0;
                Step::PhaseChanged
            }
            Phase::Refilling if levels.reservoir_empty => {
                Step::Finished(OperationResult::ReservoirEmpty)
            }
            Phase::Refilling if levels.tank_overflow => Step::Finished(OperationResult::Overflow),
            Phase::Refilling
                if levels.sump_full
                    && (self.plan.kind == OperationKind::TopOff || self.ran_ms[slot] > 0) =>
            {
                Step::Finished(OperationResult::SumpFull)
            }
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
    }

    pub fn ready_to_resume(&self, levels: Option<LevelConditions>) -> Result<(), Blocker> {
        let levels = levels.ok_or(Blocker::LevelsUnavailable)?;

        if self.phase == Phase::Refilling {
            if levels.reservoir_empty {
                return Err(Blocker::ReservoirEmpty);
            }

            if levels.tank_overflow {
                return Err(Blocker::TankOverflow);
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ChannelMode {
    Auto,
    ManualOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ChannelFault {
    OutputError,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ChannelStatus {
    pub on: bool,
    pub mode: ChannelMode,
    pub override_expires_in_s: Option<u32>,
    pub fault: Option<ChannelFault>,
    pub blocked_by: Option<Blocker>,

    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub runtime_s: u64,

    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub runtime_reset_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ChannelsStatus {
    #[serde(rename = "return")]
    pub return_pump: ChannelStatus,
    pub uv: ChannelStatus,
    pub makeup: ChannelStatus,
    pub drain: ChannelStatus,
}

impl ChannelsStatus {
    pub const fn get(&self, id: PumpId) -> ChannelStatus {
        match id {
            PumpId::Return => self.return_pump,
            PumpId::Uv => self.uv,
            PumpId::Makeup => self.makeup,
            PumpId::Drain => self.drain,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum OperationState {
    Idle,
    Draining,
    Settling,
    Refilling,
    Resuming,
    Suspended,
}

impl From<Phase> for OperationState {
    fn from(phase: Phase) -> Self {
        match phase {
            Phase::Draining => Self::Draining,
            Phase::Settling => Self::Settling,
            Phase::Refilling => Self::Refilling,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct OperationStatus {
    pub kind: Option<OperationKind>,
    pub state: OperationState,
    pub phase_remaining_s: Option<u32>,
    pub drain_ran_s: u32,
    pub settle_ran_s: u32,
    pub refill_ran_s: u32,
    pub drained_l: Option<f32>,
    pub refilled_l: Option<f32>,
}

impl OperationStatus {
    pub const IDLE: Self = Self {
        kind: None,
        state: OperationState::Idle,
        phase_remaining_s: None,
        drain_ran_s: 0,
        settle_ran_s: 0,
        refill_ran_s: 0,
        drained_l: None,
        refilled_l: None,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum SkipReason {
    Busy,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SkippedWindow {
    pub kind: OperationKind,
    pub index: u8,
    pub reason: SkipReason,
    pub blocked_by: Option<Blocker>,

    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PumpsStatus {
    pub channels: ChannelsStatus,
    pub operation: OperationStatus,
    pub last_top_off: Option<OperationRecord>,
    pub last_water_change: Option<OperationRecord>,
    pub last_skipped: Option<SkippedWindow>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PumpsTelemetry {
    #[serde(rename = "return")]
    pub return_pump: bool,
    pub uv: bool,
    pub makeup: bool,
    pub drain: bool,
    pub operation_kind: Option<OperationKind>,
    pub operation_state: OperationState,
}

impl PumpsTelemetry {
    pub const UNAVAILABLE: Self = Self {
        return_pump: false,
        uv: false,
        makeup: false,
        drain: false,
        operation_kind: None,
        operation_state: OperationState::Idle,
    };
}

#[cfg(test)]
mod tests;
