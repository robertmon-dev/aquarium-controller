use super::*;

#[tokio::test]
async fn publishes_ph_converted_from_the_probe_voltage() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(25.0),
        sump: Some(25.0),
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let expected = ph::convert(CAL.acid_volts, 25.0, &CAL)
        .expect("valid")
        .value();
    assert_close(sink.last().co2.current_ph.expect("a reading"), expected);
}

#[tokio::test]
async fn an_unavailable_probe_reports_no_ph_rather_than_a_stand_in() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().co2.current_ph, None);
}

#[tokio::test]
async fn ph_compensation_uses_the_sump_temperature_not_the_tank() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(60.0),
        sump: Some(25.0),
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let from_sump = ph::convert(CAL.acid_volts, 25.0, &CAL)
        .expect("valid")
        .value();
    let from_tank = ph::convert(CAL.acid_volts, 60.0, &CAL)
        .expect("valid")
        .value();

    let reported = sink.last().co2.current_ph.expect("a reading");
    assert_close(reported, from_sump);
    assert!(
        (reported - from_tank).abs() > 0.1,
        "the pH probe sits in the sump, so the tank temperature must not drive compensation"
    );
}

#[tokio::test]
async fn without_a_sump_reading_ph_compensation_falls_back_to_the_target() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(26.0),
        sump: None,
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let expected = ph::convert(CAL.acid_volts, config().target_celsius, &CAL)
        .expect("valid")
        .value();

    assert_close(sink.last().co2.current_ph.expect("a reading"), expected);
}

#[tokio::test]
async fn reports_electrode_wear_against_the_nominal_slope() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(CAL.neutral_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let health = sink.last().co2.ph_electrode_health.expect("a ratio");
    assert_close(health, 1.0);
}

#[tokio::test]
async fn a_worn_electrode_reports_a_ratio_below_one() {
    let sink = RecordingSink::default();
    let worn = PhCalibration {
        neutral_volts: 1.500,
        acid_volts: 1.650,
        calibrated_at_celsius: 25.0,
    };

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(1.5),
        FakeCalibration::standard().with(|cal| cal.ph = worn),
    )
    .sample()
    .await;

    let health = sink.last().co2.ph_electrode_health.expect("a ratio");
    assert!(
        health > 0.8 && health < 0.9,
        "150 mV over three pH units is about 85% of nominal, got {health}"
    );
}

#[tokio::test]
async fn republish_keeps_the_last_ph_reading() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    );

    subject.sample().await;
    subject.republish();

    assert!(sink.last().co2.current_ph.is_some());
}

#[tokio::test]
async fn ph_calibration_comes_from_the_port_not_the_static_config() {
    let sink = RecordingSink::default();
    let shifted = PhCalibration {
        neutral_volts: 1.400,
        acid_volts: 1.57748,
        calibrated_at_celsius: 25.0,
    };

    let reader = FakeReader {
        tank: Some(25.0),
        sump: Some(25.0),
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(shifted.neutral_volts),
        FakeCalibration::standard().with(|cal| cal.ph = shifted),
    )
    .sample()
    .await;

    assert_close(sink.last().co2.current_ph.expect("a reading"), 7.0);
}
