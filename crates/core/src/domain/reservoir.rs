use core::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::domain::pump::{nullable, Supervision};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

pub const MAX_OVERRIDE_S: u32 = 600;
pub const MAX_TDS_PPM: u16 = 2000;
pub const MAX_FLOW_LPH: u32 = 10_000;
pub const OVERTIME_PCT: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ReservoirChannel {
    RoValve,
    Booster,
    TapValve,
    Mixer,
}

impl ReservoirChannel {
    pub const ALL: [Self; 4] = [Self::RoValve, Self::Booster, Self::TapValve, Self::Mixer];

    pub const fn index(self) -> usize {
        match self {
            Self::RoValve => 0,
            Self::Booster => 1,
            Self::TapValve => 2,
            Self::Mixer => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::RoValve => "ro_valve",
            Self::Booster => "booster",
            Self::TapValve => "tap_valve",
            Self::Mixer => "mixer",
        }
    }

    pub const fn target(self) -> OverrideTarget {
        match self {
            Self::RoValve | Self::Booster => OverrideTarget::Ro,
            Self::TapValve => OverrideTarget::Tap,
            Self::Mixer => OverrideTarget::Mixer,
        }
    }
}

impl FromStr for ReservoirChannel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|channel| channel.name() == value)
            .ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum Inlet {
    Ro,
    Tap,
}

impl Inlet {
    pub const ALL: [Self; 2] = [Self::Ro, Self::Tap];

    pub const fn index(self) -> usize {
        match self {
            Self::Ro => 0,
            Self::Tap => 1,
        }
    }

    pub const fn valve(self) -> ReservoirChannel {
        match self {
            Self::Ro => ReservoirChannel::RoValve,
            Self::Tap => ReservoirChannel::TapValve,
        }
    }

    pub const fn target(self) -> OverrideTarget {
        match self {
            Self::Ro => OverrideTarget::Ro,
            Self::Tap => OverrideTarget::Tap,
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Ro => Self::Tap,
            Self::Tap => Self::Ro,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideTarget {
    Ro,
    Tap,
    Mixer,
}

impl OverrideTarget {
    pub const ALL: [Self; 3] = [Self::Ro, Self::Tap, Self::Mixer];

    pub const fn index(self) -> usize {
        match self {
            Self::Ro => 0,
            Self::Tap => 1,
            Self::Mixer => 2,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ro => "ro",
            Self::Tap => "tap",
            Self::Mixer => "mixer",
        }
    }

    pub const fn inlet(self) -> Option<Inlet> {
        match self {
            Self::Ro => Some(Inlet::Ro),
            Self::Tap => Some(Inlet::Tap),
            Self::Mixer => None,
        }
    }
}

impl FromStr for OverrideTarget {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|target| target.name() == value)
            .ok_or(())
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct InletFlows {
    #[serde(default)]
    pub ro: Option<u32>,

    #[serde(default)]
    pub tap: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirSettings {
    pub auto_fill: bool,
    pub refill_debounce_s: u32,
    pub empty_fill_s: u32,
    pub target_tds_ppm: Option<u16>,
    pub tolerance_ppm: u16,
    pub tap_pulse_s: u16,
    pub ro_pulse_s: u16,
    pub mix_s: u16,
    pub max_duration_s: u32,
    pub inlet_max_on_s: u32,
    pub inlet_rest_s: u32,
    pub flow_lph: InletFlows,
}

impl Default for ReservoirSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn within<T: PartialOrd>(
    value: T,
    low: T,
    high: T,
    reason: &'static str,
) -> Result<(), &'static str> {
    if value < low || value > high {
        Err(reason)
    } else {
        Ok(())
    }
}

impl ReservoirSettings {
    pub const DEFAULT: Self = Self {
        auto_fill: false,
        refill_debounce_s: 600,
        empty_fill_s: 7200,
        target_tds_ppm: None,
        tolerance_ppm: 10,
        tap_pulse_s: 5,
        ro_pulse_s: 120,
        mix_s: 60,
        max_duration_s: 14_400,
        inlet_max_on_s: 5400,
        inlet_rest_s: 900,
        flow_lph: InletFlows {
            ro: None,
            tap: None,
        },
    };

    pub fn validate(&self) -> Result<(), &'static str> {
        within(
            self.refill_debounce_s,
            60,
            7200,
            "refill_debounce_s must be between 60 and 7200",
        )?;
        within(
            self.empty_fill_s,
            600,
            43_200,
            "empty_fill_s must be between 600 and 43200",
        )?;
        if let Some(target) = self.target_tds_ppm {
            within(
                target,
                1,
                MAX_TDS_PPM,
                "target_tds_ppm must be between 1 and 2000",
            )?;
        }
        within(
            self.tolerance_ppm,
            1,
            200,
            "tolerance_ppm must be between 1 and 200",
        )?;
        within(
            self.tap_pulse_s,
            1,
            60,
            "tap_pulse_s must be between 1 and 60",
        )?;
        within(
            self.ro_pulse_s,
            10,
            1800,
            "ro_pulse_s must be between 10 and 1800",
        )?;
        within(self.mix_s, 10, 600, "mix_s must be between 10 and 600")?;
        within(
            self.max_duration_s,
            600,
            43_200,
            "max_duration_s must be between 600 and 43200",
        )?;
        within(
            self.inlet_max_on_s,
            600,
            14_400,
            "inlet_max_on_s must be between 600 and 14400",
        )?;
        within(
            self.inlet_rest_s,
            60,
            3600,
            "inlet_rest_s must be between 60 and 3600",
        )?;
        for flow in [self.flow_lph.ro, self.flow_lph.tap].into_iter().flatten() {
            within(
                flow,
                1,
                MAX_FLOW_LPH,
                "flow must be between 1 and 10000 L/h",
            )?;
        }
        if self.max_duration_s < self.empty_fill_s {
            return Err("max_duration_s must not be shorter than empty_fill_s");
        }
        Ok(())
    }

    pub const fn overtime_ms(&self) -> u64 {
        self.empty_fill_s as u64 * 1000 * OVERTIME_PCT / 100
    }

    pub const fn pulse_ms(&self, inlet: Inlet) -> u64 {
        let seconds = match inlet {
            Inlet::Ro => self.ro_pulse_s,
            Inlet::Tap => self.tap_pulse_s,
        };
        seconds as u64 * 1000
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirSettingsPatch {
    #[serde(default)]
    pub auto_fill: Option<bool>,
    #[serde(default)]
    pub refill_debounce_s: Option<u32>,
    #[serde(default)]
    pub empty_fill_s: Option<u32>,
    #[serde(default, deserialize_with = "nullable")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub target_tds_ppm: Option<Option<u16>>,
    #[serde(default)]
    pub tolerance_ppm: Option<u16>,
    #[serde(default)]
    pub tap_pulse_s: Option<u16>,
    #[serde(default)]
    pub ro_pulse_s: Option<u16>,
    #[serde(default)]
    pub mix_s: Option<u16>,
    #[serde(default)]
    pub max_duration_s: Option<u32>,
    #[serde(default)]
    pub inlet_max_on_s: Option<u32>,
    #[serde(default)]
    pub inlet_rest_s: Option<u32>,
    #[serde(default)]
    pub flow_lph: Option<InletFlows>,
}

impl ReservoirSettingsPatch {
    pub fn merge_onto(self, base: &ReservoirSettings) -> ReservoirSettings {
        ReservoirSettings {
            auto_fill: self.auto_fill.unwrap_or(base.auto_fill),
            refill_debounce_s: self.refill_debounce_s.unwrap_or(base.refill_debounce_s),
            empty_fill_s: self.empty_fill_s.unwrap_or(base.empty_fill_s),
            target_tds_ppm: self.target_tds_ppm.unwrap_or(base.target_tds_ppm),
            tolerance_ppm: self.tolerance_ppm.unwrap_or(base.tolerance_ppm),
            tap_pulse_s: self.tap_pulse_s.unwrap_or(base.tap_pulse_s),
            ro_pulse_s: self.ro_pulse_s.unwrap_or(base.ro_pulse_s),
            mix_s: self.mix_s.unwrap_or(base.mix_s),
            max_duration_s: self.max_duration_s.unwrap_or(base.max_duration_s),
            inlet_max_on_s: self.inlet_max_on_s.unwrap_or(base.inlet_max_on_s),
            inlet_rest_s: self.inlet_rest_s.unwrap_or(base.inlet_rest_s),
            flow_lph: self.flow_lph.unwrap_or(base.flow_lph),
        }
    }
}

pub fn validate_override(on: bool, duration_s: Option<u32>) -> Result<(), &'static str> {
    match (on, duration_s) {
        (_, Some(seconds)) if !(1..=MAX_OVERRIDE_S).contains(&seconds) => {
            Err("an override lasts 1 to 600 s")
        }
        (true, None) => Err("an ON override needs duration_s"),
        _ => Ok(()),
    }
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DutyLimits {
    pub max_on_ms: u64,
    pub rest_ms: u64,
}

impl ReservoirSettings {
    pub const fn duty(&self) -> DutyLimits {
        DutyLimits {
            max_on_ms: self.inlet_max_on_s as u64 * 1000,
            rest_ms: self.inlet_rest_s as u64 * 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DutyMeter {
    worked_ms: u64,
    idle_ms: u64,
    rest_left_ms: u64,
}

impl DutyMeter {
    pub fn advance(&mut self, ran: bool, elapsed_ms: u64, limits: DutyLimits) {
        if self.rest_left_ms > 0 {
            self.rest_left_ms = self.rest_left_ms.saturating_sub(elapsed_ms);
            if self.rest_left_ms == 0 {
                self.worked_ms = 0;
                self.idle_ms = 0;
            }
            return;
        }

        if ran {
            self.worked_ms += elapsed_ms;
            self.idle_ms = 0;
            if self.worked_ms >= limits.max_on_ms {
                self.rest_left_ms = limits.rest_ms;
            }
        } else {
            self.idle_ms += elapsed_ms;
            if self.idle_ms >= limits.rest_ms {
                self.worked_ms = 0;
            }
        }
    }

    pub const fn resting(&self) -> bool {
        self.rest_left_ms > 0
    }

    pub const fn rest_remaining_s(&self) -> Option<u32> {
        if self.rest_left_ms == 0 {
            None
        } else {
            Some(self.rest_left_ms.div_ceil(1000) as u32)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefillTrigger {
    dry_ms: u64,
}

impl RefillTrigger {
    pub fn advance(&mut self, conditions: Option<ReservoirConditions>, elapsed_ms: u64) {
        self.dry_ms = match conditions {
            Some(conditions) if !conditions.full => self.dry_ms.saturating_add(elapsed_ms),
            _ => 0,
        };
    }

    pub const fn due(&self, debounce_s: u32) -> bool {
        self.dry_ms >= debounce_s as u64 * 1000
    }

    pub fn reset(&mut self) {
        self.dry_ms = 0;
    }
}

pub const EMPTY_AGAIN_MS: u64 = 10_000;
pub const TDS_WAIT_MS: u64 = 30_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillTrigger {
    Manual,
    Auto,
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillResult {
    Completed,
    OffTarget,
    Overtime,
    Timeout,
    Aborted,
    TdsUnavailable,
    Leak,
    Blocked,
    InterruptedByReboot,
}

impl FillResult {
    pub const fn fault(self) -> Option<FillFault> {
        match self {
            Self::Overtime => Some(FillFault::Overtime),
            Self::Leak => Some(FillFault::Leak),
            _ => None,
        }
    }

    pub const fn latches_auto(self) -> bool {
        matches!(self, Self::Timeout | Self::TdsUnavailable | Self::Blocked)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::OffTarget => "off_target",
            Self::Overtime => "overtime",
            Self::Timeout => "timeout",
            Self::Aborted => "aborted",
            Self::TdsUnavailable => "tds_unavailable",
            Self::Leak => "leak",
            Self::Blocked => "blocked",
            Self::InterruptedByReboot => "interrupted_by_reboot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum FillFault {
    Overtime,
    Leak,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FaultRecord {
    pub fault: FillFault,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunningMarker {
    pub trigger: FillTrigger,
    pub started_at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FillRecord {
    pub trigger: FillTrigger,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub started_at: Option<u64>,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub finished_at: Option<u64>,
    pub result: FillResult,
    pub final_tds_ppm: Option<f32>,
    pub ro_s: u32,
    pub tap_s: u32,
    pub mix_s: u32,
    pub ro_pulses: u32,
    pub tap_pulses: u32,
    pub duration_s: u32,
}

impl FillRecord {
    pub const fn interrupted(marker: RunningMarker) -> Self {
        Self {
            trigger: marker.trigger,
            started_at: marker.started_at,
            finished_at: None,
            result: FillResult::InterruptedByReboot,
            final_tds_ppm: None,
            ro_s: 0,
            tap_s: 0,
            mix_s: 0,
            ro_pulses: 0,
            tap_pulses: 0,
            duration_s: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillEvent {
    pub record: FillRecord,
    pub ro_l: Option<f32>,
    pub tap_l: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillStage {
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
    pub trigger: FillTrigger,
    pub started_at: Option<u64>,
    pub stage: FillStage,
    pub finishing: bool,
    pub stage_ms: u64,
    pub wall_ms: u64,
    pub inlet_ms: [u64; 2],
    pub mix_ms: u64,
    pub pulses: [u32; 2],
    pub empty_ms: u64,
    pub last_tds: Option<f32>,
}

const fn seconds(ms: u64) -> u32 {
    (ms / 1000) as u32
}

impl Fill {
    pub const fn start(trigger: FillTrigger, started_at: Option<u64>) -> Self {
        Self {
            trigger,
            started_at,
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

    pub fn advance(&mut self, input: FillInput, settings: &ReservoirSettings) -> FillStep {
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
        if !conditions.full && self.inlet_ms[0] + self.inlet_ms[1] > settings.overtime_ms() {
            return FillStep::Finished(FillResult::Overtime);
        }
        if self.wall_ms > settings.max_duration_s as u64 * 1000 {
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
            FillStage::Priming => self.prime(conditions, settings),
            FillStage::Mixing => {
                if input.mixed {
                    self.stage_ms += elapsed;
                }
                if self.stage_ms >= settings.mix_s as u64 * 1000 {
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
                        self.after_measuring(tds, conditions, settings)
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
                    self.reach_full(settings)
                } else if settings.target_tds_ppm.is_some()
                    && self.stage_ms >= settings.pulse_ms(inlet)
                {
                    self.enter(FillStage::Mixing)
                } else {
                    FillStep::Continue
                }
            }
        }
    }

    fn prime(&mut self, conditions: ReservoirConditions, settings: &ReservoirSettings) -> FillStep {
        if conditions.full {
            return self.reach_full(settings);
        }
        if conditions.empty {
            return FillStep::Continue;
        }
        match settings.target_tds_ppm {
            None => self.dose(Inlet::Ro),
            Some(_) => self.enter(FillStage::Mixing),
        }
    }

    fn reach_full(&mut self, settings: &ReservoirSettings) -> FillStep {
        if settings.target_tds_ppm.is_none() {
            return FillStep::Finished(FillResult::Completed);
        }
        self.finishing = true;
        self.enter(FillStage::Mixing)
    }

    fn after_measuring(
        &mut self,
        tds: f32,
        conditions: ReservoirConditions,
        settings: &ReservoirSettings,
    ) -> FillStep {
        let Some(target) = settings.target_tds_ppm else {
            return FillStep::Finished(FillResult::Completed);
        };
        let target = f32::from(target);
        let tolerance = f32::from(settings.tolerance_ppm);

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
            ro_s: seconds(self.inlet_ms[Inlet::Ro.index()]),
            tap_s: seconds(self.inlet_ms[Inlet::Tap.index()]),
            mix_s: seconds(self.mix_ms),
            ro_pulses: self.pulses[Inlet::Ro.index()],
            tap_pulses: self.pulses[Inlet::Tap.index()],
            duration_s: seconds(self.wall_ms),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ChannelMode {
    Auto,
    ManualOn,
    ManualOff,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirChannelStatus {
    pub on: bool,
    pub mode: ChannelMode,
    pub override_expires_in_s: Option<u32>,
    pub blocked_by: Option<ReservoirBlocker>,
    pub output_error: bool,
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub runtime_s: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirChannels {
    pub ro_valve: ReservoirChannelStatus,
    pub booster: ReservoirChannelStatus,
    pub tap_valve: ReservoirChannelStatus,
    pub mixer: ReservoirChannelStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FillStatus {
    pub phase: FillPhase,
    pub dosing: Option<Inlet>,
    pub trigger: FillTrigger,
    pub elapsed_s: u32,
    pub tds_ppm: Option<f32>,
    pub ro_s: u32,
    pub tap_s: u32,
}

impl Fill {
    pub const fn status(&self) -> FillStatus {
        FillStatus {
            phase: self.phase(),
            dosing: self.dosing(),
            trigger: self.trigger,
            elapsed_s: seconds(self.wall_ms),
            tds_ppm: self.last_tds,
            ro_s: seconds(self.inlet_ms[0]),
            tap_s: seconds(self.inlet_ms[1]),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum SkipReason {
    HeldAfterReboot,
    AutoLatched,
    Fault,
    Leak,
    SupervisionUnavailable,
    WaterChange,
    Storage,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SkippedFill {
    pub reason: SkipReason,
    #[cfg_attr(feature = "ts-export", ts(type = "number | null"))]
    pub at: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct InletRest {
    pub ro: Option<u32>,
    pub tap: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirStatus {
    pub channels: ReservoirChannels,
    pub fill: Option<FillStatus>,
    pub conditions: Option<ReservoirConditions>,
    pub water_change_active: bool,
    pub target_tds_ppm: Option<u16>,
    pub fault: Option<FaultRecord>,
    pub held_after_reboot: bool,
    pub auto_latched: bool,
    pub invalidation_pending: bool,
    pub supervision: Supervision,
    pub rest_remaining_s: InletRest,
    pub last: Option<FillRecord>,
    pub last_skipped: Option<SkippedFill>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirTelemetry {
    pub ro_valve: bool,
    pub booster: bool,
    pub tap_valve: bool,
    pub mixer: bool,
    pub phase: FillPhase,
    pub tds_ppm: Option<f32>,
    pub target_tds_ppm: Option<u16>,
    pub elapsed_s: u32,
    pub water_change_active: bool,
    pub held_after_reboot: bool,
    pub auto_latched: bool,
    pub fault: Option<FillFault>,
    pub supervision: Supervision,
    pub last_result: Option<FillResult>,
}

impl ReservoirTelemetry {
    pub const UNAVAILABLE: Self = Self {
        ro_valve: false,
        booster: false,
        tap_valve: false,
        mixer: false,
        phase: FillPhase::Idle,
        tds_ppm: None,
        target_tds_ppm: None,
        elapsed_s: 0,
        water_change_active: false,
        held_after_reboot: false,
        auto_latched: false,
        fault: None,
        supervision: Supervision::Off,
        last_result: None,
    };
}

impl Default for ReservoirTelemetry {
    fn default() -> Self {
        Self::UNAVAILABLE
    }
}

#[cfg(test)]
mod tests;
