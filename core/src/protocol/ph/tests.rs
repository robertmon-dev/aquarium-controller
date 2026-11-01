use super::*;

const CAL: PhCalibration = PhCalibration {
    neutral_volts: 1.500,
    acid_volts: 1.67748,
    calibrated_at_celsius: 25.0,
};

const INVERTED: PhCalibration = PhCalibration {
    neutral_volts: 1.500,
    acid_volts: 1.32252,
    calibrated_at_celsius: 25.0,
};

fn ph_at(volts: f32, celsius: f32, cal: &PhCalibration) -> f32 {
    convert(volts, celsius, cal).expect("valid reading").value()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected ~{expected}, got {actual}"
    );
}

#[test]
fn the_neutral_point_reads_exactly_seven() {
    assert_close(ph_at(CAL.neutral_volts, 25.0, &CAL), 7.0);
}

#[test]
fn the_acid_point_reads_exactly_four() {
    assert_close(ph_at(CAL.acid_volts, 25.0, &CAL), 4.0);
}

#[test]
fn halfway_between_the_points_reads_five_and_a_half() {
    let midpoint = (CAL.neutral_volts + CAL.acid_volts) / 2.0;
    assert_close(ph_at(midpoint, 25.0, &CAL), 5.5);
}

#[test]
fn extrapolates_past_the_neutral_point_into_alkaline() {
    let span = CAL.acid_volts - CAL.neutral_volts;
    assert_close(ph_at(CAL.neutral_volts - span / 3.0, 25.0, &CAL), 8.0);
}

#[test]
fn board_polarity_does_not_change_the_result() {
    let normal = ph_at(CAL.acid_volts, 25.0, &CAL);
    let inverted = ph_at(INVERTED.acid_volts, 25.0, &INVERTED);

    assert_close(normal, inverted);
    assert_close(inverted, 4.0);
}

#[test]
fn the_calibration_temperature_applies_no_correction() {
    let uncompensated = ph_at(CAL.acid_volts, 25.0, &CAL);
    assert_close(uncompensated, 4.0);
}

#[test]
fn a_warmer_sample_reads_closer_to_neutral_for_the_same_voltage() {
    let cold = ph_at(CAL.acid_volts, 25.0, &CAL);
    let warm = ph_at(CAL.acid_volts, 45.0, &CAL);

    assert!(
        warm > cold,
        "a bigger slope at higher temperature means fewer pH units per volt: \
         25 C gave {cold}, 45 C gave {warm}"
    );
}

#[test]
fn a_colder_sample_reads_further_from_neutral_for_the_same_voltage() {
    let reference = ph_at(CAL.acid_volts, 25.0, &CAL);
    let cold = ph_at(CAL.acid_volts, 5.0, &CAL);

    assert!(cold < reference, "25 C gave {reference}, 5 C gave {cold}");
}

#[test]
fn two_identical_calibration_points_are_rejected() {
    let dead = PhCalibration {
        neutral_volts: 1.5,
        acid_volts: 1.5,
        calibrated_at_celsius: 25.0,
    };

    assert_eq!(
        dead.validate(),
        Err(PhError::InvalidCalibration(
            "the two buffer points are too close; same buffer measured twice, or a dead electrode"
        ))
    );
    assert!(convert(1.5, 25.0, &dead).is_err());
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
        "10 mV over three pH units is dead"
    );
}

#[test]
fn a_non_finite_voltage_is_rejected() {
    assert_eq!(convert(f32::NAN, 25.0, &CAL), Err(PhError::InvalidVoltage));
    assert_eq!(
        convert(f32::INFINITY, 25.0, &CAL),
        Err(PhError::InvalidVoltage)
    );
}

#[test]
fn a_non_finite_temperature_is_rejected() {
    assert!(convert(CAL.neutral_volts, f32::NAN, &CAL).is_err());
}

#[test]
fn absolute_zero_temperature_is_rejected_rather_than_dividing_by_zero() {
    assert!(convert(CAL.neutral_volts, -273.15, &CAL).is_err());
    assert!(convert(CAL.neutral_volts, -300.0, &CAL).is_err());
}

#[test]
fn a_voltage_far_off_the_scale_is_rejected() {
    assert!(
        convert(5.0, 25.0, &CAL).is_err(),
        "a shorted input must not produce a plausible-looking pH"
    );
}

#[test]
fn volts_per_ph_is_the_span_over_three_units() {
    assert_close(CAL.volts_per_ph(), 0.05916);
}

#[test]
fn volts_per_ph_ignores_board_polarity() {
    assert_close(INVERTED.volts_per_ph(), 0.05916);
}
