use serde::{Deserialize, Serialize};

pub const NEUTRAL_PH: f32 = 7.0;
pub const ACID_PH: f32 = 4.0;
pub const CALIBRATION_SPAN_PH: f32 = NEUTRAL_PH - ACID_PH;

const MIN_CALIBRATION_SPAN_VOLTS: f32 = 0.030;
const ABSOLUTE_ZERO_CELSIUS: f32 = -273.15;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PhCalibration {
    pub neutral_volts: f32,
    pub acid_volts: f32,
    pub calibrated_at_celsius: f32,
}

impl PhCalibration {
    pub fn volts_per_ph(&self) -> f32 {
        ((self.acid_volts - self.neutral_volts) / CALIBRATION_SPAN_PH).abs()
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.neutral_volts.is_finite()
            || !self.acid_volts.is_finite()
            || !self.calibrated_at_celsius.is_finite()
        {
            return Err("calibration holds a non-finite value");
        }

        if (self.acid_volts - self.neutral_volts).abs() < MIN_CALIBRATION_SPAN_VOLTS {
            return Err(
                "the two buffer points are too close; same buffer measured twice, or a dead electrode",
            );
        }

        if self.calibrated_at_celsius <= ABSOLUTE_ZERO_CELSIUS {
            return Err("calibration temperature is at or below absolute zero");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct Calibration {
    pub ph: PhCalibration,
    pub makeup_tank_celsius: f32,
    pub tds_factor: f32,
}

impl Calibration {
    pub const MIN_MAKEUP_CELSIUS: f32 = 0.0;
    pub const MAX_MAKEUP_CELSIUS: f32 = 40.0;
    pub const MIN_TDS_FACTOR: f32 = 0.1;
    pub const MAX_TDS_FACTOR: f32 = 10.0;

    pub fn validate(&self) -> Result<(), &'static str> {
        self.ph.validate()?;

        if !self.makeup_tank_celsius.is_finite()
            || !(Self::MIN_MAKEUP_CELSIUS..=Self::MAX_MAKEUP_CELSIUS)
                .contains(&self.makeup_tank_celsius)
        {
            return Err("makeup tank temperature must be between 0 and 40 degrees Celsius");
        }

        if !self.tds_factor.is_finite()
            || !(Self::MIN_TDS_FACTOR..=Self::MAX_TDS_FACTOR).contains(&self.tds_factor)
        {
            return Err("TDS factor must be between 0.1 and 10.0");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct CalibrationPatch {
    #[serde(default)]
    pub ph: Option<PhCalibration>,

    #[serde(default)]
    pub makeup_tank_celsius: Option<f32>,

    #[serde(default)]
    pub tds_factor: Option<f32>,
}

impl CalibrationPatch {
    pub fn merge_onto(&self, base: Calibration) -> Calibration {
        Calibration {
            ph: self.ph.unwrap_or(base.ph),
            makeup_tank_celsius: self.makeup_tank_celsius.unwrap_or(base.makeup_tank_celsius),
            tds_factor: self.tds_factor.unwrap_or(base.tds_factor),
        }
    }
}

#[cfg(test)]
mod tests;
