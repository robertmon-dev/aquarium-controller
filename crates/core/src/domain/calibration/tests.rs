use super::*;

const PH: PhCalibration = PhCalibration {
    neutral_volts: 1.500,
    acid_volts: 1.67748,
    calibrated_at_celsius: 25.0,
};

fn valid() -> Calibration {
    Calibration {
        ph: PH,
        makeup_tank_celsius: 25.0,
        tds_factor: 1.0,
    }
}

#[test]
fn volts_per_ph_is_the_span_over_three_units() {
    assert!((PH.volts_per_ph() - 0.05916).abs() < 0.0001);
}

#[test]
fn board_polarity_does_not_change_volts_per_ph() {
    let inverted = PhCalibration {
        neutral_volts: 1.500,
        acid_volts: 1.32252,
        calibrated_at_celsius: 25.0,
    };

    assert!((inverted.volts_per_ph() - 0.05916).abs() < 0.0001);
}

#[test]
fn a_sane_calibration_validates() {
    assert_eq!(valid().validate(), Ok(()));
}

#[test]
fn two_identical_buffer_points_are_rejected() {
    let dead = PhCalibration {
        neutral_volts: 1.5,
        acid_volts: 1.5,
        calibrated_at_celsius: 25.0,
    };

    assert!(dead.validate().is_err());
}

#[test]
fn a_collapsed_span_is_rejected_even_when_the_points_differ() {
    let worn = PhCalibration {
        neutral_volts: 1.500,
        acid_volts: 1.510,
        calibrated_at_celsius: 25.0,
    };

    assert!(
        worn.validate().is_err(),
        "10 mV over three pH units is a dead electrode"
    );
}

#[test]
fn a_non_finite_calibration_point_is_rejected() {
    let broken = PhCalibration {
        neutral_volts: f32::NAN,
        acid_volts: 1.67748,
        calibrated_at_celsius: 25.0,
    };

    assert!(broken.validate().is_err());
}

#[test]
fn a_calibration_temperature_at_absolute_zero_is_rejected() {
    let impossible = PhCalibration {
        neutral_volts: 1.500,
        acid_volts: 1.67748,
        calibrated_at_celsius: -273.15,
    };

    assert!(impossible.validate().is_err());
}

#[test]
fn a_makeup_tank_temperature_outside_the_sane_band_is_rejected() {
    let mut cold = valid();
    cold.makeup_tank_celsius = -1.0;
    assert!(cold.validate().is_err());

    let mut hot = valid();
    hot.makeup_tank_celsius = 41.0;
    assert!(hot.validate().is_err());
}

#[test]
fn both_ends_of_the_makeup_tank_band_are_accepted() {
    let mut cold = valid();
    cold.makeup_tank_celsius = Calibration::MIN_MAKEUP_CELSIUS;
    assert_eq!(cold.validate(), Ok(()));

    let mut warm = valid();
    warm.makeup_tank_celsius = Calibration::MAX_MAKEUP_CELSIUS;
    assert_eq!(warm.validate(), Ok(()));
}

#[test]
fn a_tds_factor_outside_the_sane_band_is_rejected() {
    let mut tiny = valid();
    tiny.tds_factor = 0.05;
    assert!(tiny.validate().is_err());

    let mut huge = valid();
    huge.tds_factor = 11.0;
    assert!(huge.validate().is_err());

    let mut broken = valid();
    broken.tds_factor = f32::NAN;
    assert!(broken.validate().is_err());
}

#[test]
fn an_empty_patch_changes_nothing() {
    let patch = CalibrationPatch {
        ph: None,
        makeup_tank_celsius: None,
        tds_factor: None,
    };

    assert_eq!(patch.merge_onto(valid()), valid());
}

#[test]
fn a_patch_replaces_only_the_fields_it_carries() {
    let patch = CalibrationPatch {
        ph: None,
        makeup_tank_celsius: Some(23.0),
        tds_factor: None,
    };

    let merged = patch.merge_onto(valid());

    assert_eq!(merged.makeup_tank_celsius, 23.0);
    assert_eq!(merged.ph, PH);
    assert_eq!(merged.tds_factor, 1.0);
}

#[test]
fn a_patch_replaces_all_three_ph_points_at_once() {
    let fresh = PhCalibration {
        neutral_volts: 1.480,
        acid_volts: 1.660,
        calibrated_at_celsius: 21.0,
    };

    let patch = CalibrationPatch {
        ph: Some(fresh),
        makeup_tank_celsius: None,
        tds_factor: None,
    };

    assert_eq!(patch.merge_onto(valid()).ph, fresh);
}
