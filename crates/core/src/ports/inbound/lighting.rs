use crate::domain::lighting::{
    LightingOverride, LightingPalette, LightingPalettePatch, LightingTarget,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightingPaletteError {
    Invalid(&'static str),
    NotPersisted(&'static str),
}

pub trait LightingControl {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>);
    fn clear_override(&self);
    fn current_override(&self) -> Option<LightingOverride>;
    fn palette(&self) -> LightingPalette;
    fn apply_palette(
        &self,
        patch: LightingPalettePatch,
    ) -> Result<LightingPalette, LightingPaletteError>;
}
