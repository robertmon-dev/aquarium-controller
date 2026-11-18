use crate::domain::calibration::{PhCalibration, ACID_PH, NEUTRAL_PH};
use crate::domain::metrics::Ph;

const KELVIN_OFFSET: f32 = 273.15;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum PhError {
    #[error("ADC reading is not a finite voltage")]
    InvalidVoltage,

    #[error("compensation temperature is not usable: {0}")]
    InvalidTemperature(&'static str),

    #[error("calibration is unusable: {0}")]
    InvalidCalibration(&'static str),

    #[error("pH reading out of range: {0}")]
    OutOfRange(&'static str),

    #[error("pH probe is unavailable: {0}")]
    Unavailable(&'static str),
}

pub fn convert(volts: f32, celsius: f32, cal: &PhCalibration) -> Result<Ph, PhError> {
    if !volts.is_finite() {
        return Err(PhError::InvalidVoltage);
    }

    if !celsius.is_finite() {
        return Err(PhError::InvalidTemperature("temperature is not a number"));
    }

    let kelvin = celsius + KELVIN_OFFSET;
    if kelvin <= 0.0 {
        return Err(PhError::InvalidTemperature(
            "temperature is at or below absolute zero",
        ));
    }

    cal.validate().map_err(PhError::InvalidCalibration)?;

    let ph_per_volt = (ACID_PH - NEUTRAL_PH) / (cal.acid_volts - cal.neutral_volts);
    let calibration_kelvin = cal.calibrated_at_celsius + KELVIN_OFFSET;
    let compensated = ph_per_volt * calibration_kelvin / kelvin;

    Ph::new(NEUTRAL_PH + (volts - cal.neutral_volts) * compensated).map_err(PhError::OutOfRange)
}

#[cfg(test)]
mod tests;
