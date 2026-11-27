use crate::domain::sump_light::SumpLightStatus;

pub trait SumpLightControl {
    fn status(&self) -> SumpLightStatus;
    fn set(&self, on: bool, duration_s: Option<u32>) -> Result<SumpLightStatus, &'static str>;
}
