mod channels;
mod decision;
#[cfg(test)]
mod fixtures;
mod operation;
mod settings;
mod status;

pub use channels::{
    validate_override, OverrideError, PumpId, MAX_RETURN_OVERRIDE_S, MAX_UV_OVERRIDE_S,
    MIN_RETURN_OVERRIDE_S, MIN_UV_OVERRIDE_S,
};
pub use decision::{
    decide, Blocker, ChannelDecision, DecisionInput, LevelConditions, Phase, Supervision,
    STARVED_CLEAR_S, STARVED_LOCKOUTS, STARVED_WINDOW_S,
};
pub use operation::{
    litres, phase_timeout_s, refill_timeout_s, resume_verdict, Checkpoint, LastOperations,
    Operation, OperationEvent, OperationKind, OperationPlan, OperationRecord, OperationResult,
    ResumeVerdict, RuntimeCounters, Step, StoredOverride, MIN_REFILL_SHARE_PCT,
    REFILL_TIMEOUT_GRACE_S,
};
pub(crate) use settings::nullable;
pub use settings::{
    validate_top_off, FlowRates, PumpSettings, PumpSettingsPatch, TopOffWindow, WaterChangePlan,
    WaterChangeWindow, DEFAULT_RESUME_WINDOW_S, DEFAULT_SETTLE_S, DEFAULT_WATER_CHANGE_GAP_H,
    MAX_CRON_LEN, MAX_DRAIN_S, MAX_FLOW_LPH, MAX_REFILL_S, MAX_RESERVOIR_L, MAX_RESUME_WINDOW_S,
    MAX_SETTLE_S, MAX_TOP_OFF_S, MAX_WATER_CHANGE_GAP_H, MAX_WINDOWS,
};
pub use status::{
    ChannelFault, ChannelMode, ChannelStatus, ChannelsStatus, OperationState, OperationStatus,
    PumpsStatus, PumpsTelemetry, SkipReason, SkippedWindow,
};
