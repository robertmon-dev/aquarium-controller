use crate::domain::calibration::Calibration;

pub trait CalibrationSource {
    fn calibration(&self) -> Calibration;
}

pub trait CalibrationPersistence {
    fn load(&self) -> Option<Calibration>;
    fn store(&self, value: &Calibration) -> Result<(), &'static str>;
}
