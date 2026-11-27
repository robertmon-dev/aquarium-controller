use super::*;

#[tokio::test]
async fn sampling_publishes_the_raw_probe_voltages() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog {
            ph_volts: Some(1.52),
            tds_volts: Some(0.91),
        },
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let volts = sink.last_volts();

    assert_close(volts.ph.expect("a pH voltage"), 1.52);
    assert_close(volts.tds.expect("a TDS voltage"), 0.91);
}

#[tokio::test]
async fn the_published_voltages_carry_the_same_timestamp_as_the_snapshot() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(1.5),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(
        sink.last_volts().sampled_at_epoch_s,
        sink.last().timestamp_epoch_s,
        "a client comparing the two would otherwise see the voltages as stale or from the future"
    );
}

#[tokio::test]
async fn a_probe_that_failed_publishes_no_voltage_rather_than_zero() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(0.91),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let volts = sink.last_volts();

    assert_eq!(volts.ph, None, "zero volts is a reading, absence is not");
    assert_close(volts.tds.expect("a TDS voltage"), 0.91);
}

#[tokio::test]
async fn republishing_does_not_restamp_the_probe_voltages() {
    let sink = RecordingSink::default();
    let subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(1.5),
        FakeCalibration::standard(),
    );
    let mut subject = subject;

    subject.sample().await;
    subject.republish();

    assert_eq!(sink.count(), 2, "republish does emit a snapshot");
    assert_eq!(
        sink.volts_count(),
        1,
        "a lighting change is not a new probe reading, so the voltages must keep their timestamp"
    );
}

#[tokio::test]
async fn a_tds_voltage_the_board_cannot_produce_reaches_telemetry_as_no_reading() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(3.3),
        FakeCalibration::standard().with(|cal| cal.tds_factor = 0.1),
    )
    .sample()
    .await;

    assert_eq!(sink.last().tds.ppm, None);
    assert_eq!(sink.last().tds.ec_us_cm, None);
    assert_close(sink.last_volts().tds.expect("the raw voltage"), 3.3);
}

#[tokio::test]
async fn a_failed_ph_conversion_blanks_only_ph_and_lends_no_voltage_to_tds() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog {
            ph_volts: Some(1.52),
            tds_volts: Some(0.91),
        },
        FakeCalibration::standard(),
    );

    subject.sample().await;
    subject.sources.analog.ph_volts = None;
    subject.sources.analog.tds_volts = Some(1.2);
    subject.sample().await;

    let volts = sink.last_volts();
    let snapshot = sink.last();
    let expected = tds::convert(1.2, 25.0, 1.0).expect("valid");

    assert_eq!(volts.ph, None);
    assert_close(volts.tds.expect("a TDS voltage"), 1.2);
    assert_eq!(snapshot.co2.current_ph, None);
    assert_close(snapshot.tds.ppm.expect("a reading"), expected.ppm.as_ppm());
}

#[tokio::test]
async fn both_analog_channels_recover_after_the_bus_drops_out() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog {
            ph_volts: Some(1.52),
            tds_volts: Some(0.91),
        },
        FakeCalibration::standard(),
    );

    subject.sample().await;
    subject.sources.analog = FakeAnalog::nothing_attached();
    subject.sample().await;

    let dropped = sink.last_volts();
    assert_eq!((dropped.ph, dropped.tds), (None, None));
    assert_eq!(sink.last().co2.current_ph, None);
    assert_eq!(sink.last().tds.ppm, None);

    subject.sources.analog = FakeAnalog {
        ph_volts: Some(1.5),
        tds_volts: Some(1.1),
    };
    subject.sample().await;

    let recovered = sink.last_volts();
    assert_close(recovered.ph.expect("pH is back"), 1.5);
    assert_close(recovered.tds.expect("TDS is back"), 1.1);
    assert!(sink.last().co2.current_ph.is_some());
    assert!(sink.last().tds.ppm.is_some());
}
