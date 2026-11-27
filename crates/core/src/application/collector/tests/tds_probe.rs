use super::*;

#[tokio::test]
async fn a_tds_reading_reaches_telemetry_in_ppm_and_conductivity() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(2.16),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let expected = tds::convert(2.16, 25.0, 1.0).expect("valid");
    let published = sink.last();

    assert_close(published.tds.ppm.expect("a reading"), expected.ppm.as_ppm());
    assert_close(
        published.tds.ec_us_cm.expect("a reading"),
        expected.ec_us_cm,
    );
}

#[tokio::test]
async fn an_unavailable_tds_probe_reports_nothing_rather_than_a_stand_in() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().tds.ppm, None);
    assert_eq!(sink.last().tds.ec_us_cm, None);
}

#[tokio::test]
async fn tds_compensation_uses_the_makeup_tank_temperature_from_calibration() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(1.0),
        FakeCalibration::standard().with(|cal| cal.makeup_tank_celsius = 15.0),
    )
    .sample()
    .await;

    let expected = tds::convert(1.0, 15.0, 1.0).expect("valid");
    let at_reference = tds::convert(1.0, 25.0, 1.0).expect("valid");
    let reported = sink.last().tds.ppm.expect("a reading");

    assert_close(reported, expected.ppm.as_ppm());
    assert!(
        (reported - at_reference.ppm.as_ppm()).abs() > 1.0,
        "the makeup tank temperature from calibration must drive compensation, \
         not the 25 C reference"
    );
}

#[tokio::test]
async fn the_tds_factor_from_calibration_scales_the_reading() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(1.0),
        FakeCalibration::standard().with(|cal| cal.tds_factor = 1.5),
    )
    .sample()
    .await;

    let expected = tds::convert(1.0, 25.0, 1.5).expect("valid");

    assert_close(
        sink.last().tds.ppm.expect("a reading"),
        expected.ppm.as_ppm(),
    );
}

#[tokio::test]
async fn a_dead_ph_probe_does_not_suppress_the_tds_reading() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(1.5),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let published = sink.last();

    assert_eq!(published.co2.current_ph, None);
    assert!(published.tds.ppm.is_some());
}

#[tokio::test(start_paused = true)]
async fn a_sample_publishes_the_tds_reading_with_its_measurement_time() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(2.16),
        FakeCalibration::standard(),
    );
    let before = Instant::now();

    subject.sample().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    subject.republish();

    let samples = sink.tds.lock().unwrap().clone();
    assert_eq!(
        samples.len(),
        1,
        "a republish must not refresh the TDS sample"
    );
    let expected = tds::convert(2.16, 25.0, 1.0).expect("valid").ppm.as_ppm();
    assert_close(samples[0].ppm.expect("a reading"), expected);
    assert!(samples[0].measured_at >= before);
    assert!(samples[0].measured_at <= before + Duration::from_secs(1));
}

#[tokio::test]
async fn a_missing_tds_probe_publishes_an_empty_sample() {
    let sink = RecordingSink::default();
    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(sink.tds.lock().unwrap()[0].ppm, None);
}
