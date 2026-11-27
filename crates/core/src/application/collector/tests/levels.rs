use super::*;

#[tokio::test]
async fn publishes_the_water_levels_from_the_source() {
    let sink = RecordingSink::default();
    with_levels(
        &sink,
        FakeLevels::wet(&[WaterLevelRole::SumpPump, WaterLevelRole::MainTank]),
    )
    .sample()
    .await;

    let s = sink.last();
    assert!(s.water_level.sump_pump.wet);
    assert!(s.water_level.main_tank.wet);
    assert!(!s.water_level.sump_mechanical.wet);
    assert!(!s.water_level.sump_biological.wet);
    assert!(s.water_level.sensors_healthy);
}

#[tokio::test]
async fn a_faulty_level_sensor_reaches_telemetry() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::faulty(WaterLevelRole::SumpMechanical))
        .sample()
        .await;

    let s = sink.last();
    assert!(s.water_level.sump_mechanical.faulty);
    assert!(!s.water_level.sensors_healthy);
}

#[tokio::test]
async fn the_sump_is_low_for_the_ato_when_the_pump_chamber_is_dry() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::all_dry()).sample().await;

    assert!(sink.last().ato.sump_water_low);
}

#[tokio::test]
async fn the_sump_is_not_low_for_the_ato_while_the_pump_chamber_is_wet() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::SumpPump]))
        .sample()
        .await;

    let s = sink.last();
    assert!(!s.ato.sump_water_low);
    assert!(!s.ato.pump_active);
}

#[tokio::test]
async fn levels_before_the_first_sample_are_reported_as_unavailable() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::never_sampled())
        .sample()
        .await;

    let s = sink.last();
    assert!(!s.water_level.available);
    assert!(!s.water_level.sensors_healthy);
    assert!(!s.water_level.sump_pump.wet);
    assert!(!s.ato.sump_water_low);
    assert!(!s.ato.reservoir_empty);
}

#[tokio::test]
async fn a_real_all_dry_reading_is_available_and_drives_the_ato_flags() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::all_dry()).sample().await;

    let s = sink.last();
    assert!(s.water_level.available);
    assert!(s.water_level.sensors_healthy);
    assert!(s.ato.sump_water_low);
    assert!(s.ato.reservoir_empty);
}

#[tokio::test]
async fn levels_from_a_stopped_sampler_are_reported_as_unavailable() {
    let sink = RecordingSink::default();
    with_levels(
        &sink,
        FakeLevels::stale(
            FakeLevels::wet(&[WaterLevelRole::SumpPump])
                .0
                .unwrap()
                .levels,
        ),
    )
    .sample()
    .await;

    let s = sink.last();
    assert!(!s.water_level.available);
    assert!(!s.water_level.sensors_healthy);
    assert!(!s.water_level.sump_pump.wet);
    assert!(!s.ato.sump_water_low);
}

#[tokio::test]
async fn a_filled_reservoir_is_not_empty_for_the_ato() {
    let sink = RecordingSink::default();
    with_levels(
        &sink,
        FakeLevels::wet(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]),
    )
    .sample()
    .await;

    assert!(!sink.last().ato.reservoir_empty);
}

async fn ato_with(
    pumps: PumpsTelemetry,
    last_top_off: Option<OperationRecord>,
) -> TankTelemetryDto {
    let sink = RecordingSink::default();
    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::wet(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(pumps, last_top_off),
            reservoir: NoReservoir,
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        &sink,
        config(),
    )
    .sample()
    .await;
    sink.last()
}

fn top_off_record(refill_ran_s: u32) -> OperationRecord {
    OperationRecord {
        kind: OperationKind::TopOff,
        started_at: Some(1_750_000_000),
        drain_ran_s: 0,
        settle_ran_s: 0,
        refill_ran_s,
        result: OperationResult::Completed,
        resumed: false,
        finished_at: Some(1_750_000_000 + u64::from(refill_ran_s)),
    }
}

#[tokio::test]
async fn the_ato_card_follows_a_running_top_off_and_the_last_fill() {
    let topping_off = PumpsTelemetry {
        makeup: true,
        operation_kind: Some(OperationKind::TopOff),
        operation_state: OperationState::Refilling,
        ..PumpsTelemetry::UNAVAILABLE
    };

    let s = ato_with(topping_off, Some(top_off_record(42))).await;
    assert!(s.ato.pump_active);
    assert_eq!(s.ato.last_fill_duration_ms, 42_000);

    let interlocked = PumpsTelemetry {
        makeup: false,
        ..topping_off
    };
    assert!(!ato_with(interlocked, None).await.ato.pump_active);
    assert_eq!(
        ato_with(interlocked, None).await.ato.last_fill_duration_ms,
        0
    );
}

#[tokio::test]
async fn a_water_change_refill_is_not_an_ato_top_off() {
    let refilling = PumpsTelemetry {
        makeup: true,
        operation_kind: Some(OperationKind::WaterChange),
        operation_state: OperationState::Refilling,
        ..PumpsTelemetry::UNAVAILABLE
    };

    let s = ato_with(refilling, Some(top_off_record(42))).await;
    assert!(!s.ato.pump_active);
    assert_eq!(s.ato.last_fill_duration_ms, 42_000);
}

#[tokio::test]
async fn the_reserve_chamber_does_not_drive_the_ato_low_flag() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::SumpPump]))
        .sample()
        .await;

    assert!(!sink.last().ato.sump_water_low);
    assert!(!sink.last().water_level.sump_reserve.wet);
}

#[tokio::test]
async fn republish_keeps_the_last_water_levels() {
    let sink = RecordingSink::default();
    let mut subject = with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::MainTank]));

    subject.sample().await;
    subject.republish();

    let s = sink.last();
    assert!(s.water_level.main_tank.wet);
}
