use std::collections::HashMap;

use super::*;
use crate::domain::calibration::{Calibration, PhCalibration};
use crate::domain::heater::{HeaterSettings, ZoneSettings};

fn heater() -> HeaterSettings {
    HeaterSettings {
        tank: ZoneSettings {
            target_celsius: 27.0,
            hysteresis_celsius: 0.3,
        },
        sump: ZoneSettings {
            target_celsius: 26.5,
            hysteresis_celsius: 0.5,
        },
        cutoff_celsius: 30.0,
    }
}

fn calibration() -> Calibration {
    Calibration {
        ph: PhCalibration {
            neutral_volts: 1.52,
            acid_volts: 2.03,
            calibrated_at_celsius: 25.0,
        },
        makeup_tank_celsius: 21.5,
        tds_factor: 0.5,
    }
}

fn legacy_heater_keys(settings: &HeaterSettings) -> HashMap<&'static str, u32> {
    HashMap::from([
        (
            legacy::HEATER_TANK_TARGET,
            settings.tank.target_celsius.to_bits(),
        ),
        (
            legacy::HEATER_TANK_HYSTERESIS,
            settings.tank.hysteresis_celsius.to_bits(),
        ),
        (
            legacy::HEATER_SUMP_TARGET,
            settings.sump.target_celsius.to_bits(),
        ),
        (
            legacy::HEATER_SUMP_HYSTERESIS,
            settings.sump.hysteresis_celsius.to_bits(),
        ),
        (legacy::HEATER_CUTOFF, settings.cutoff_celsius.to_bits()),
    ])
}

fn legacy_calibration_keys(value: &Calibration) -> HashMap<&'static str, u32> {
    HashMap::from([
        (legacy::PH_NEUTRAL, value.ph.neutral_volts.to_bits()),
        (legacy::PH_ACID, value.ph.acid_volts.to_bits()),
        (legacy::PH_CELSIUS, value.ph.calibrated_at_celsius.to_bits()),
        (legacy::MAKEUP_CELSIUS, value.makeup_tank_celsius.to_bits()),
        (legacy::TDS_FACTOR, value.tds_factor.to_bits()),
    ])
}

#[test]
fn heater_settings_round_trip_through_one_record() {
    let bytes = encode_heater(&heater()).expect("encodes");

    assert_eq!(decode_heater(&bytes), Some(heater()));
}

#[test]
fn calibration_round_trips_through_one_record() {
    let bytes = encode_calibration(&calibration()).expect("encodes");

    assert_eq!(decode_calibration(&bytes), Some(calibration()));
}

#[test]
fn values_without_a_short_decimal_form_survive_bit_for_bit() {
    let mut settings = heater();
    settings.tank.hysteresis_celsius = 0.1 + 0.2;
    let bytes = encode_heater(&settings).expect("encodes");

    let restored = decode_heater(&bytes).expect("decodes");

    assert_eq!(
        restored.tank.hysteresis_celsius.to_bits(),
        settings.tank.hysteresis_celsius.to_bits()
    );
}

#[test]
fn the_record_names_its_version() {
    let bytes = encode_heater(&heater()).expect("encodes");
    let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");

    assert_eq!(value["version"], 1);
}

#[test]
fn a_record_from_another_version_is_ignored() {
    let bytes = encode_heater(&heater()).expect("encodes");
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    value["version"] = serde_json::json!(2);

    assert_eq!(decode_heater(&serde_json::to_vec(&value).unwrap()), None);
}

#[test]
fn a_record_that_fails_validation_is_ignored() {
    let mut settings = heater();
    settings.cutoff_celsius = 20.0;
    let bytes = encode_heater(&settings).expect("encodes");

    assert_eq!(decode_heater(&bytes), None);
}

#[test]
fn a_truncated_record_is_ignored() {
    let bytes = encode_calibration(&calibration()).expect("encodes");

    assert_eq!(decode_calibration(&bytes[..bytes.len() / 2]), None);
}

#[test]
fn the_legacy_heater_layout_is_read_when_complete() {
    let keys = legacy_heater_keys(&heater());

    let restored = legacy_heater(Some(1), |key| keys.get(key).copied());

    assert_eq!(restored, Some(heater()));
}

#[test]
fn the_legacy_heater_layout_needs_its_version_and_every_key() {
    let mut keys = legacy_heater_keys(&heater());

    assert_eq!(legacy_heater(None, |key| keys.get(key).copied()), None);
    assert_eq!(legacy_heater(Some(2), |key| keys.get(key).copied()), None);

    keys.remove(legacy::HEATER_CUTOFF);
    assert_eq!(legacy_heater(Some(1), |key| keys.get(key).copied()), None);
}

#[test]
fn the_legacy_calibration_layout_is_read_and_validated() {
    let keys = legacy_calibration_keys(&calibration());
    assert_eq!(
        legacy_calibration(Some(1), |key| keys.get(key).copied()),
        Some(calibration())
    );

    let mut broken = calibration();
    broken.tds_factor = -1.0;
    let keys = legacy_calibration_keys(&broken);
    assert_eq!(
        legacy_calibration(Some(1), |key| keys.get(key).copied()),
        None
    );
}

#[test]
fn a_present_record_wins_and_the_legacy_layout_is_not_consulted() {
    let bytes = encode_heater(&heater()).expect("encodes");

    let restored = restore(Some(&bytes), decode_heater, || -> Option<HeaterSettings> {
        panic!("the legacy layout must not be read once a record exists")
    });

    assert_eq!(restored, Restored::Current(heater()));
}

#[test]
fn without_a_record_the_legacy_layout_is_migrated() {
    let restored = restore(None, decode_heater, || Some(heater()));

    assert_eq!(restored, Restored::Migrated(heater()));
}

#[test]
fn without_either_layout_nothing_is_restored() {
    let restored = restore(None, decode_heater, || None);

    assert_eq!(restored, Restored::Missing);
}

#[test]
fn a_damaged_record_does_not_fall_back_to_older_values() {
    let restored = restore(Some(b"{\"version\":1"), decode_heater, || Some(heater()));

    assert_eq!(restored, Restored::Missing);
}
