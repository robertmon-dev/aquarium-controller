use crate::domain::light::LightingOverride;

pub trait LightingOverrideSource {
    fn active_override(&self) -> Option<LightingOverride>;
}
