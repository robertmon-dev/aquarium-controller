use crate::domain::calibration::{Calibration, CalibrationPatch};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalibrationError {
    Invalid(&'static str),
    NotPersisted(&'static str),
}

pub trait CalibrationControl {
    fn current(&self) -> Calibration;
    fn apply(&self, patch: CalibrationPatch) -> Result<Calibration, CalibrationError>;
}
