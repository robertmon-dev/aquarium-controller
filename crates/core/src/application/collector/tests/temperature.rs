use super::*;

#[tokio::test]
async fn publishes_both_sensor_readings() {
    let sink = RecordingSink::default();
    collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.temperature.tank_celsius, Some(26.5));
    assert_eq!(s.temperature.sump_celsius, Some(26.0));
    assert!(s.temperature.sensors_healthy);
}

#[tokio::test]
async fn one_dead_sensor_does_not_lose_the_other() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(26.5),
        sump: None,
        trigger_fails: false,
        conversions: 0,
    };
    collector(reader, FakeClock::synced_at_noon_in_june(), &sink)
        .sample()
        .await;

    let s = sink.last();
    assert_eq!(s.temperature.tank_celsius, Some(26.5));
    assert_eq!(s.temperature.sump_celsius, None);
    assert!(!s.temperature.sensors_healthy);
}

#[tokio::test]
async fn bus_failure_still_publishes_a_snapshot() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(26.5),
        sump: Some(26.0),
        trigger_fails: true,
        conversions: 0,
    };
    collector(reader, FakeClock::synced_at_noon_in_june(), &sink)
        .sample()
        .await;

    assert_eq!(sink.count(), 1, "a bus failure must not stall telemetry");
    let s = sink.last();
    assert_eq!(s.temperature.tank_celsius, None);
    assert_eq!(s.temperature.sump_celsius, None);
    assert_eq!(
        s.system_health.free_heap_kb, 180,
        "system health keeps working"
    );
}

#[tokio::test]
async fn out_of_safe_range_reading_is_reported_not_dropped() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(21.0),
        sump: Some(26.0),
        trigger_fails: false,
        conversions: 0,
    };
    collector(reader, FakeClock::synced_at_noon_in_june(), &sink)
        .sample()
        .await;

    assert_eq!(
        sink.last().temperature.tank_celsius,
        Some(21.0),
        "an unsafe temperature MUST reach telemetry"
    );
}

#[tokio::test]
async fn republish_does_not_touch_the_sensor_bus() {
    let sink = RecordingSink::default();
    let mut c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );

    c.sample().await;
    let after_sample = c.sources.temperature.conversions;

    c.republish();
    c.republish();

    assert_eq!(sink.count(), 3, "every call publishes a snapshot");
    assert_eq!(
        c.sources.temperature.conversions, after_sample,
        "a burst of API requests MUST NOT hammer the 1-Wire bus"
    );
}

#[tokio::test]
async fn republish_reuses_the_last_readings() {
    let sink = RecordingSink::default();
    let mut c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );

    c.sample().await;
    c.republish();

    assert_eq!(sink.last().temperature.tank_celsius, Some(26.5));
}

#[tokio::test(start_paused = true)]
async fn only_a_sample_publishes_temperatures_stamped_when_measured() {
    let sink = RecordingSink::default();
    let mut c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );
    let measured_at = tokio::time::Instant::now();

    c.sample().await;
    tokio::time::advance(std::time::Duration::from_millis(1_900)).await;
    c.republish();

    assert_eq!(
        *sink.temperatures.lock().unwrap(),
        vec![TemperatureSample {
            tank_celsius: Some(26.5),
            sump_celsius: Some(26.0),
            measured_at,
        }]
    );
}

#[tokio::test]
async fn republish_before_any_sample_reports_no_readings() {
    let sink = RecordingSink::default();
    let c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );

    c.republish();

    let s = sink.last();
    assert_eq!(
        s.temperature.tank_celsius, None,
        "an empty cache reports no reading instead of faking data"
    );
    assert!(!s.temperature.sensors_healthy);
}
