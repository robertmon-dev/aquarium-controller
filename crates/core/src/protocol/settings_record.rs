use serde::{Deserialize, Serialize};

use crate::domain::calibration::{Calibration, PhCalibration};
use crate::domain::heater::{HeaterSettings, ZoneSettings};

pub const HEATER_RECORD_KEY: &str = "heat_rec";
pub const CALIBRATION_RECORD_KEY: &str = "cal_rec";

const RECORD_VERSION: u8 = 1;
const LEGACY_LAYOUT_VERSION: u8 = 1;

pub mod legacy {
    pub const HEATER_VERSION: &str = "heat_ver";
    pub const HEATER_TANK_TARGET: &str = "heat_tank_t";
    pub const HEATER_TANK_HYSTERESIS: &str = "heat_tank_h";
    pub const HEATER_SUMP_TARGET: &str = "heat_sump_t";
    pub const HEATER_SUMP_HYSTERESIS: &str = "heat_sump_h";
    pub const HEATER_CUTOFF: &str = "heat_cutoff";

    pub const CALIBRATION_VERSION: &str = "cal_ver";
    pub const PH_NEUTRAL: &str = "ph_neutral";
    pub const PH_ACID: &str = "ph_acid";
    pub const PH_CELSIUS: &str = "ph_cal_c";
    pub const MAKEUP_CELSIUS: &str = "makeup_c";
    pub const TDS_FACTOR: &str = "tds_k";
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Restored<T> {
    Current(T),
    Migrated(T),
    Missing,
}

#[derive(Serialize, Deserialize)]
struct HeaterRecord {
    version: u8,
    tank_target: u32,
    tank_hysteresis: u32,
    sump_target: u32,
    sump_hysteresis: u32,
    cutoff: u32,
}

#[derive(Serialize, Deserialize)]
struct CalibrationRecord {
    version: u8,
    ph_neutral: u32,
    ph_acid: u32,
    ph_celsius: u32,
    makeup_celsius: u32,
    tds_factor: u32,
}

fn encode<T: Serialize>(record: &T) -> Result<Vec<u8>, &'static str> {
    serde_json::to_vec(record).map_err(|_| "the settings record could not be encoded")
}

pub fn encode_heater(settings: &HeaterSettings) -> Result<Vec<u8>, &'static str> {
    encode(&HeaterRecord {
        version: RECORD_VERSION,
        tank_target: settings.tank.target_celsius.to_bits(),
        tank_hysteresis: settings.tank.hysteresis_celsius.to_bits(),
        sump_target: settings.sump.target_celsius.to_bits(),
        sump_hysteresis: settings.sump.hysteresis_celsius.to_bits(),
        cutoff: settings.cutoff_celsius.to_bits(),
    })
}

pub fn decode_heater(bytes: &[u8]) -> Option<HeaterSettings> {
    let record = serde_json::from_slice::<HeaterRecord>(bytes).ok()?;

    if record.version != RECORD_VERSION {
        return None;
    }

    let settings = HeaterSettings {
        tank: ZoneSettings {
            target_celsius: f32::from_bits(record.tank_target),
            hysteresis_celsius: f32::from_bits(record.tank_hysteresis),
        },
        sump: ZoneSettings {
            target_celsius: f32::from_bits(record.sump_target),
            hysteresis_celsius: f32::from_bits(record.sump_hysteresis),
        },
        cutoff_celsius: f32::from_bits(record.cutoff),
    };

    settings.validate().ok().map(|()| settings)
}

pub fn encode_calibration(value: &Calibration) -> Result<Vec<u8>, &'static str> {
    encode(&CalibrationRecord {
        version: RECORD_VERSION,
        ph_neutral: value.ph.neutral_volts.to_bits(),
        ph_acid: value.ph.acid_volts.to_bits(),
        ph_celsius: value.ph.calibrated_at_celsius.to_bits(),
        makeup_celsius: value.makeup_tank_celsius.to_bits(),
        tds_factor: value.tds_factor.to_bits(),
    })
}

pub fn decode_calibration(bytes: &[u8]) -> Option<Calibration> {
    let record = serde_json::from_slice::<CalibrationRecord>(bytes).ok()?;

    if record.version != RECORD_VERSION {
        return None;
    }

    let value = Calibration {
        ph: PhCalibration {
            neutral_volts: f32::from_bits(record.ph_neutral),
            acid_volts: f32::from_bits(record.ph_acid),
            calibrated_at_celsius: f32::from_bits(record.ph_celsius),
        },
        makeup_tank_celsius: f32::from_bits(record.makeup_celsius),
        tds_factor: f32::from_bits(record.tds_factor),
    };

    value.validate().ok().map(|()| value)
}

pub fn legacy_heater(
    version: Option<u8>,
    read: impl Fn(&str) -> Option<u32>,
) -> Option<HeaterSettings> {
    if version? != LEGACY_LAYOUT_VERSION {
        return None;
    }

    let float = |key| read(key).map(f32::from_bits);
    let settings = HeaterSettings {
        tank: ZoneSettings {
            target_celsius: float(legacy::HEATER_TANK_TARGET)?,
            hysteresis_celsius: float(legacy::HEATER_TANK_HYSTERESIS)?,
        },
        sump: ZoneSettings {
            target_celsius: float(legacy::HEATER_SUMP_TARGET)?,
            hysteresis_celsius: float(legacy::HEATER_SUMP_HYSTERESIS)?,
        },
        cutoff_celsius: float(legacy::HEATER_CUTOFF)?,
    };

    settings.validate().ok().map(|()| settings)
}

pub fn legacy_calibration(
    version: Option<u8>,
    read: impl Fn(&str) -> Option<u32>,
) -> Option<Calibration> {
    if version? != LEGACY_LAYOUT_VERSION {
        return None;
    }

    let float = |key| read(key).map(f32::from_bits);
    let value = Calibration {
        ph: PhCalibration {
            neutral_volts: float(legacy::PH_NEUTRAL)?,
            acid_volts: float(legacy::PH_ACID)?,
            calibrated_at_celsius: float(legacy::PH_CELSIUS)?,
        },
        makeup_tank_celsius: float(legacy::MAKEUP_CELSIUS)?,
        tds_factor: float(legacy::TDS_FACTOR)?,
    };

    value.validate().ok().map(|()| value)
}

pub fn restore<T>(
    blob: Option<&[u8]>,
    decode: impl FnOnce(&[u8]) -> Option<T>,
    legacy: impl FnOnce() -> Option<T>,
) -> Restored<T> {
    match blob {
        Some(bytes) => decode(bytes).map_or(Restored::Missing, Restored::Current),
        None => legacy().map_or(Restored::Missing, Restored::Migrated),
    }
}

#[cfg(test)]
mod tests;
