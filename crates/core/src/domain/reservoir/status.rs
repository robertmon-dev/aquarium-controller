use serde::{Deserialize, Serialize};

use super::channels::Inlet;
use super::conditions::ReservoirConditions;
use super::decision::ReservoirBlocker;
use super::fill::{Fill, FillPhase};
use super::outcome::{FaultRecord, FillFault, FillRecord, FillResult, FillTrigger};
use crate::domain::pump::Supervision;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename = "ReservoirChannelMode"))]
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
pub struct ReservoirChannels<T = ReservoirChannelStatus> {
    pub ro_valve: T,
    pub booster: T,
    pub tap_valve: T,
    pub mixer: T,
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

impl From<&Fill> for FillStatus {
    fn from(fill: &Fill) -> Self {
        Self {
            phase: fill.phase(),
            dosing: fill.dosing(),
            trigger: fill.trigger(),
            elapsed_s: fill.elapsed_s(),
            tds_ppm: fill.last_tds(),
            ro_s: fill.inlet_s(Inlet::Ro),
            tap_s: fill.inlet_s(Inlet::Tap),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, rename = "ReservoirSkipReason"))]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirChannelTelemetry {
    pub on: bool,
    pub blocked_by: Option<ReservoirBlocker>,
    pub output_error: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirTelemetry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub channels: Option<ReservoirChannels<ReservoirChannelTelemetry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub dosing_water: Option<Inlet>,
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
        channels: None,
        dosing_water: None,
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
