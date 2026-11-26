use crate::domain::lighting::{ChannelLevels, LightingOverride};

pub trait LightingOverrideSource {
    fn active_override(&self) -> Option<LightingOverride>;
}

pub trait LightingOutputs {
    fn set_levels(&mut self, levels: ChannelLevels) -> Result<(), &'static str>;
}
