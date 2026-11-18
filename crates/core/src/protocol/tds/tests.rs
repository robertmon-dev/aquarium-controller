use super::*;
use crate::domain::calibration::Calibration;

fn ppm_at(volts: f32, celsius: f32) -> f32 {
    convert(volts, celsius, 1.0)
        .expect("valid reading")
        .ppm
        .as_ppm()
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1.0,
        "expected ~{expected}, got {actual}"
    );
}

#[test]
fn the_rated_maximum_sits_near_two_point_one_six_volts() {
    assert_close(ppm_at(2.16, 25.0), 1001.0);
}

#[test]
fn the_board_ceiling_reads_just_past_the_rated_range() {
    assert_close(ppm_at(2.30, 25.0), 1121.0);
}

#[test]
fn the_reference_temperature_applies_no_compensation() {
    let compensated = convert(1.0, 25.0, 1.0).expect("valid reading");
    let raw_polynomial = 133.42 - 255.86 + 857.39;

    assert_close(compensated.ec_us_cm, raw_polynomial);
}

#[test]
fn a_warmer_sample_reads_lower_for_the_same_voltage() {
    let reference = ppm_at(2.16, 25.0);
    let warm = ppm_at(2.16, 30.0);

    assert!(
        warm < reference,
        "warmer water conducts better, so the same voltage means less dissolved solids: \
         25 C gave {reference}, 30 C gave {warm}"
    );
}

#[test]
fn a_colder_sample_reads_higher_for_the_same_voltage() {
    let reference = ppm_at(1.5, 25.0);
    let cold = ppm_at(1.5, 15.0);

    assert!(cold > reference, "25 C gave {reference}, 15 C gave {cold}");
}

#[test]
fn ppm_is_half_the_conductivity() {
    let reading = convert(1.5, 25.0, 1.0).expect("valid reading");

    assert_close(reading.ppm.as_ppm(), reading.ec_us_cm * 0.5);
}

#[test]
fn the_factor_scales_the_result_linearly() {
    let plain = ppm_at(1.0, 25.0);
    let doubled = convert(1.0, 25.0, 2.0).expect("valid reading").ppm.as_ppm();

    assert_close(doubled, plain * 2.0);
}

#[test]
fn a_probe_in_air_reads_zero_rather_than_a_negative_number() {
    let reading = convert(-0.01, 25.0, 1.0).expect("noise around zero is not a fault");

    assert_eq!(reading.ppm.as_ppm(), 0.0);
    assert_eq!(reading.ec_us_cm, 0.0);
}

#[test]
fn a_voltage_the_board_cannot_produce_is_rejected() {
    assert!(
        matches!(convert(2.5, 25.0, 1.0), Err(TdsError::OutOfRange(_))),
        "a shorted input must not produce a plausible-looking TDS"
    );
}

#[test]
fn a_non_finite_voltage_is_rejected() {
    assert_eq!(convert(f32::NAN, 25.0, 1.0), Err(TdsError::InvalidVoltage));
    assert_eq!(
        convert(f32::INFINITY, 25.0, 1.0),
        Err(TdsError::InvalidVoltage)
    );
}

#[test]
fn a_non_finite_temperature_is_rejected() {
    assert!(matches!(
        convert(1.5, f32::NAN, 1.0),
        Err(TdsError::InvalidTemperature(_))
    ));
}

#[test]
fn the_temperature_that_collapses_the_divisor_is_rejected() {
    assert!(
        matches!(
            convert(1.5, -25.0, 1.0),
            Err(TdsError::InvalidTemperature(_))
        ),
        "at -25 C the compensation divisor is exactly zero"
    );
    assert!(matches!(
        convert(1.5, -40.0, 1.0),
        Err(TdsError::InvalidTemperature(_))
    ));
}

#[test]
fn a_non_positive_factor_is_rejected() {
    assert!(matches!(
        convert(1.5, 25.0, 0.0),
        Err(TdsError::InvalidCalibration(_))
    ));
    assert!(matches!(
        convert(1.5, 25.0, -1.0),
        Err(TdsError::InvalidCalibration(_))
    ));
    assert!(matches!(
        convert(1.5, 25.0, f32::NAN),
        Err(TdsError::InvalidCalibration(_))
    ));
}

#[test]
fn neither_end_of_the_validated_temperature_band_collapses_the_divisor() {
    for celsius in [
        Calibration::MIN_MAKEUP_CELSIUS,
        Calibration::MAX_MAKEUP_CELSIUS,
    ] {
        let reading = convert(0.5, celsius, 1.0);

        assert!(
            reading.is_ok(),
            "a mid-scale voltage must convert anywhere in the validated band, \
             but {celsius} C gave {reading:?}"
        );
    }
}

#[test]
fn a_cold_sample_inflates_the_reading_enough_to_leave_the_board_range() {
    let warm = convert(1.5, 25.0, 1.0).expect("valid at the reference temperature");
    let cold = convert(1.5, Calibration::MIN_MAKEUP_CELSIUS, 1.0);

    assert!(warm.ppm.as_ppm() > 0.0);
    assert!(
        matches!(cold, Err(TdsError::OutOfRange(_))),
        "at 0 C the divisor halves, so the same voltage implies a TDS the board cannot \
         represent; reporting it as out of range beats reporting a wrong number, got {cold:?}"
    );
}
