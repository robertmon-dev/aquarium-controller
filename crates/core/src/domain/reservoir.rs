mod channels;
mod conditions;
mod debounce;
mod decision;
mod duty;
mod fill;
#[cfg(test)]
mod fixtures;
mod outcome;
mod settings;
mod status;

pub use channels::{validate_override, Inlet, OverrideTarget, ReservoirChannel, MAX_OVERRIDE_S};
pub use conditions::ReservoirConditions;
pub use debounce::RefillDebounce;
pub use decision::{
    admit, decide, global_blocker, inlet_blocker, ChannelDecision, DecisionInput, Demand,
    ReservoirBlocker, BOOSTER_LEAD_MS,
};
pub use duty::{DutyLimits, DutyMeter};
pub use fill::{Fill, FillInput, FillPhase, FillPlan, FillStep, EMPTY_AGAIN_MS, TDS_WAIT_MS};
pub use outcome::{
    FaultRecord, FillEvent, FillFault, FillRecord, FillResult, FillTrigger, RunningMarker,
};
pub use settings::{
    InletFlows, ReservoirSettings, ReservoirSettingsPatch, MAX_FLOW_LPH, MAX_TDS_PPM, OVERTIME_PCT,
};
pub use status::{
    ChannelMode, FillStatus, InletRest, ReservoirChannelStatus, ReservoirChannelTelemetry,
    ReservoirChannels, ReservoirStatus, ReservoirTelemetry, SkipReason, SkippedFill,
};
