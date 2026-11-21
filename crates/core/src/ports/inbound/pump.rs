pub use crate::domain::pump::OverrideError;
use crate::domain::pump::{
    Blocker, ChannelStatus, OperationKind, PumpId, PumpSettings, PumpSettingsPatch, PumpsStatus,
    WaterChangePlan,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpSettingsError {
    Invalid(&'static str),
    NotPersisted(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartError {
    Invalid(&'static str),
    Conflict,
    Blocked(Blocker),
}

pub trait PumpControl {
    fn status(&self) -> PumpsStatus;
    fn settings(&self) -> PumpSettings;
    fn apply_settings(&self, patch: PumpSettingsPatch) -> Result<PumpSettings, PumpSettingsError>;
    fn set_override(
        &self,
        id: PumpId,
        duration_s: Option<u32>,
    ) -> Result<ChannelStatus, OverrideError>;
    fn clear_override(&self, id: PumpId) -> Result<(), OverrideError>;
    fn reset_runtime(&self, id: PumpId);
    fn start_top_off(&self, duration_s: u32) -> Result<(), StartError>;
    fn start_water_change(&self, plan: WaterChangePlan) -> Result<(), StartError>;
    fn resume(&self, kind: OperationKind) -> Result<(), StartError>;
    fn cancel(&self, kind: OperationKind);
}
