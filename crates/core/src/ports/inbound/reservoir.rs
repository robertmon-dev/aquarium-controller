use crate::domain::reservoir::{
    OverrideTarget, ReservoirBlocker, ReservoirChannel, ReservoirSettings, ReservoirSettingsPatch,
    ReservoirStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservoirSettingsError {
    Invalid(&'static str),
    NotPersisted(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillStartError {
    Conflict,
    Blocked(ReservoirBlocker),
    NotPersisted(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FillCancelError {
    NotPersisted(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservoirOverrideError {
    Invalid(&'static str),
    Busy,
    Blocked(ReservoirBlocker),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultClearError {
    Blocked(ReservoirBlocker),
    NotPersisted(&'static str),
}

pub trait ReservoirControl {
    fn status(&self) -> ReservoirStatus;
    fn settings(&self) -> ReservoirSettings;
    fn apply_settings(
        &self,
        patch: ReservoirSettingsPatch,
    ) -> Result<ReservoirSettings, ReservoirSettingsError>;
    fn set_override(
        &self,
        target: OverrideTarget,
        on: bool,
        duration_s: Option<u32>,
    ) -> Result<ReservoirStatus, ReservoirOverrideError>;
    fn clear_override(&self, target: OverrideTarget);
    fn reset_runtime(&self, channel: ReservoirChannel);
    fn start_fill(&self) -> Result<(), FillStartError>;
    fn cancel_fill(&self) -> Result<(), FillCancelError>;
    fn clear_fault(&self) -> Result<(), FaultClearError>;
}
