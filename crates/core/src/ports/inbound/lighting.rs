use crate::domain::light::{LightingOverride, LightingTarget};

pub trait LightingControl {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>);
    fn clear_override(&self);
    fn current_override(&self) -> Option<LightingOverride>;
}
