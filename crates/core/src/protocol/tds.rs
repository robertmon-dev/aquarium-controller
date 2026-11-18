use crate::domain::metrics::Tds;

pub const PPM_PER_MICROSIEMENS: f32 = 0.5;
pub const REFERENCE_CELSIUS: f32 = 25.0;

const COMPENSATION_PER_CELSIUS: f32 = 0.02;
const CUBIC_COEFFICIENT: f32 = 133.42;
const SQUARE_COEFFICIENT: f32 = 255.86;
const LINEAR_COEFFICIENT: f32 = 857.39;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum TdsError {
    #[error("ADC reading is not a finite voltage")]
    InvalidVoltage,

    #[error("compensation temperature is not usable: {0}")]
    InvalidTemperature(&'static str),

    #[error("calibration is unusable: {0}")]
    InvalidCalibration(&'static str),

    #[error("TDS reading out of range: {0}")]
    OutOfRange(&'static str),

    #[error("TDS probe is unavailable: {0}")]
    Unavailable(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TdsReading {
    pub ppm: Tds,
    pub ec_us_cm: f32,
}

pub fn convert(volts: f32, celsius: f32, factor: f32) -> Result<TdsReading, TdsError> {
    if !volts.is_finite() {
        return Err(TdsError::InvalidVoltage);
    }

    if !celsius.is_finite() {
        return Err(TdsError::InvalidTemperature("temperature is not a number"));
    }

    if !factor.is_finite() || factor <= 0.0 {
        return Err(TdsError::InvalidCalibration(
            "the TDS factor must be a positive number",
        ));
    }

    let compensation = 1.0 + COMPENSATION_PER_CELSIUS * (celsius - REFERENCE_CELSIUS);
    if compensation <= 0.0 {
        return Err(TdsError::InvalidTemperature(
            "temperature collapses the compensation divisor",
        ));
    }

    let compensated = volts / compensation;
    let ec_us_cm = (CUBIC_COEFFICIENT * compensated.powi(3)
        - SQUARE_COEFFICIENT * compensated.powi(2)
        + LINEAR_COEFFICIENT * compensated)
        .max(0.0)
        * factor;

    Tds::new(ec_us_cm * PPM_PER_MICROSIEMENS)
        .map(|ppm| TdsReading { ppm, ec_us_cm })
        .map_err(TdsError::OutOfRange)
}

#[cfg(test)]
mod tests;
