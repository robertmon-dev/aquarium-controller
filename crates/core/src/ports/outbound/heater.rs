use crate::domain::heater::{HeaterId, HeaterSettings, HeatersStatus};

pub trait HeaterOutputs {
    fn set(&mut self, id: HeaterId, on: bool) -> Result<(), &'static str>;
}

pub trait HeaterPersistence {
    fn load(&self) -> Option<HeaterSettings>;
    fn store(&self, value: &HeaterSettings) -> Result<(), &'static str>;
}

pub trait HeaterStatusSource {
    fn heater_status(&self) -> HeatersStatus;
}
