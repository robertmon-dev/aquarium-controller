use super::*;

#[tokio::test]
async fn carries_system_health_and_timestamp() {
    let sink = RecordingSink::default();
    collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.timestamp_epoch_s, 1_750_000_000);
    assert_eq!(s.system_health.uptime_seconds, 3600);
    assert_eq!(s.system_health.wifi_rssi_dbm, Some(-57));
}

#[tokio::test]
async fn an_unreadable_wifi_signal_is_reported_as_absent_not_zero() {
    let sink = RecordingSink::default();

    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: OfflineHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
            reservoir: NoReservoir,
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        &sink,
        config(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().system_health.wifi_rssi_dbm, None);
}

#[tokio::test]
async fn unsynced_clock_reports_zero_timestamp() {
    let sink = RecordingSink::default();
    collector(FakeReader::healthy(), FakeClock::unsynced(), &sink)
        .sample()
        .await;

    let s = sink.last();
    assert_eq!(s.timestamp_epoch_s, 0, "0 is a sentinel, not a faked date");
    assert_eq!(s.system_health.uptime_seconds, 12, "uptime is always known");
}
