use serde::{Deserialize, Serialize};

use super::channels::PumpId;
use super::decision::{Blocker, Phase, Supervision};
use super::operation::{OperationKind, OperationRecord};

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
    Storage,
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
    pub circulation_starved: bool,
    pub level_sensors_suspect: bool,
    pub return_held_after_drain: bool,
    pub invalidation_pending: bool,
    pub supervision: Supervision,
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

    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, as = "Option<bool>"))]
    pub circulation_starved: bool,

    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, as = "Option<bool>"))]
    pub level_sensors_suspect: bool,

    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, as = "Option<bool>"))]
    pub return_held_after_drain: bool,

    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, as = "Option<bool>"))]
    pub invalidation_pending: bool,

    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional, as = "Option<Supervision>"))]
    pub supervision: Supervision,
}

impl PumpsTelemetry {
    pub const UNAVAILABLE: Self = Self {
        return_pump: false,
        uv: false,
        makeup: false,
        drain: false,
        operation_kind: None,
        operation_state: OperationState::Idle,
        circulation_starved: false,
        level_sensors_suspect: false,
        return_held_after_drain: false,
        invalidation_pending: false,
        supervision: Supervision::Off,
    };
}

#[cfg(test)]
mod tests;
