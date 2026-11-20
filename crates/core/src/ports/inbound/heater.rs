use crate::domain::heater::{
    HeaterId, HeaterSettings, HeaterSettingsPatch, HeaterStatus, HeatersStatus, OverrideState,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeaterSettingsError {
    Invalid(&'static str),
    NotPersisted(&'static str),
}

pub trait HeaterControl {
    fn status(&self) -> HeatersStatus;
    fn settings(&self) -> HeaterSettings;
    fn apply_settings(
        &self,
        patch: HeaterSettingsPatch,
    ) -> Result<HeaterSettings, HeaterSettingsError>;
    fn set_override(
        &self,
        id: HeaterId,
        state: OverrideState,
        duration_s: Option<u32>,
    ) -> Result<HeaterStatus, &'static str>;
    fn clear_override(&self, id: HeaterId);
}
