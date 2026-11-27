use crate::domain::sump_light::SumpLightStatus;

pub trait SumpLightOutputs {
    fn set(&mut self, on: bool) -> Result<(), &'static str>;
}

pub trait SumpLightStatusSource {
    fn sump_light_status(&self) -> SumpLightStatus;
}
