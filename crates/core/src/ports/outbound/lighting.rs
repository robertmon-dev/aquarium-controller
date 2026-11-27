use crate::domain::lighting::{ChannelLevels, LightingPalette};
use crate::domain::metrics::LightingMetricsDto;

pub trait LightingOutputs {
    fn set_levels(&mut self, levels: ChannelLevels) -> Result<(), &'static str>;
}

pub trait LightingPersistence {
    fn load(&self) -> Option<LightingPalette>;
    fn store(&self, value: &LightingPalette) -> Result<(), &'static str>;
}

pub trait LightingStatusSource {
    fn lighting_status(&self) -> LightingMetricsDto;
}
