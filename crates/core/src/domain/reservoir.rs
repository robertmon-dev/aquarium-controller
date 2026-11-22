mod channels;
mod conditions;
mod decision;
mod duty;
mod fill;
#[cfg(test)]
mod fixtures;
mod outcome;
mod settings;
mod status;
mod trigger;

pub use channels::{Inlet, OverrideTarget, ReservoirChannel};
pub use conditions::{ReservoirBlocker, ReservoirConditions};
pub use decision::{decide, ChannelDecision, DecisionInput, Demand, BOOSTER_LEAD_MS};
pub use duty::{DutyLimits, DutyMeter};
pub use fill::{
    Fill, FillInput, FillPhase, FillStage, FillStep, FillTrigger, EMPTY_AGAIN_MS, TDS_WAIT_MS,
};
pub use outcome::{FaultRecord, FillEvent, FillFault, FillRecord, FillResult, RunningMarker};
pub use settings::{
    validate_override, InletFlows, ReservoirSettings, ReservoirSettingsPatch, MAX_FLOW_LPH,
    MAX_OVERRIDE_S, MAX_TDS_PPM, OVERTIME_PCT,
};
pub use status::{
    ChannelMode, FillStatus, InletRest, ReservoirChannelStatus, ReservoirChannels, ReservoirStatus,
    ReservoirTelemetry, SkipReason, SkippedFill,
};
pub use trigger::RefillTrigger;
