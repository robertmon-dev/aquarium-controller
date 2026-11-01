use crate::domain::metrics::Ph;

pub const NEUTRAL_PH: f32 = 7.0;
pub const ACID_PH: f32 = 4.0;
pub const CALIBRATION_SPAN_PH: f32 = NEUTRAL_PH - ACID_PH;

const KELVIN_OFFSET: f32 = 273.15;
const MIN_CALIBRATION_SPAN_VOLTS: f32 = 0.030;

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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhCalibration {
    pub neutral_volts: f32,
    pub acid_volts: f32,
    pub calibrated_at_celsius: f32,
}

impl PhCalibration {
    pub fn volts_per_ph(&self) -> f32 {
        ((self.acid_volts - self.neutral_volts) / CALIBRATION_SPAN_PH).abs()
    }

    pub fn validate(&self) -> Result<(), PhError> {
        if !self.neutral_volts.is_finite()
            || !self.acid_volts.is_finite()
            || !self.calibrated_at_celsius.is_finite()
        {
            return Err(PhError::InvalidCalibration(
                "calibration holds a non-finite value",
            ));
        }

        if (self.acid_volts - self.neutral_volts).abs() < MIN_CALIBRATION_SPAN_VOLTS {
            return Err(PhError::InvalidCalibration(
                "the two buffer points are too close; same buffer measured twice, or a dead electrode",
            ));
        }

        if self.calibrated_at_celsius + KELVIN_OFFSET <= 0.0 {
            return Err(PhError::InvalidCalibration(
                "calibration temperature is at or below absolute zero",
            ));
        }

        Ok(())
    }
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

    cal.validate()?;

    let ph_per_volt = (ACID_PH - NEUTRAL_PH) / (cal.acid_volts - cal.neutral_volts);
    let calibration_kelvin = cal.calibrated_at_celsius + KELVIN_OFFSET;
    let compensated = ph_per_volt * calibration_kelvin / kelvin;

    Ph::new(NEUTRAL_PH + (volts - cal.neutral_volts) * compensated).map_err(PhError::OutOfRange)
}

#[cfg(test)]
mod tests;
