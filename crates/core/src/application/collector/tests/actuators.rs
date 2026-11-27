use super::*;

#[tokio::test]
async fn the_heater_status_is_copied_into_the_snapshot() {
    let sink = RecordingSink::default();
    let heaters = HeatersStatus {
        tank: HeaterStatus {
            active: true,
            mode: HeaterMode::ManualOn,
            fault: None,
            override_expires_in_s: Some(300),
            target_celsius: 27.0,
        },
        sump: HeaterStatus::unavailable(26.0),
    };

    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(heaters),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
            reservoir: NoReservoir,
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        &sink,
        config(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().heaters, heaters);
}

#[tokio::test]
async fn the_pump_state_is_copied_into_the_snapshot() {
    let sink = RecordingSink::default();
    let pumps = PumpsTelemetry {
        return_pump: true,
        uv: false,
        makeup: true,
        drain: false,
        operation_kind: Some(OperationKind::TopOff),
        operation_state: OperationState::Refilling,
        circulation_starved: true,
        level_sensors_suspect: true,
        return_held_after_drain: true,
        invalidation_pending: true,
        supervision: Supervision::Unavailable,
    };

    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(pumps, None),
            reservoir: NoReservoir,
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        &sink,
        config(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().pumps, pumps);
}

struct FixedReservoir(ReservoirTelemetry);

impl ReservoirStatusSource for FixedReservoir {
    fn reservoir_telemetry(&self) -> ReservoirTelemetry {
        self.0
    }
}

#[tokio::test]
async fn telemetry_carries_the_reservoir_section() {
    let sink = RecordingSink::default();
    let reservoir = ReservoirTelemetry {
        ro_valve: true,
        phase: FillPhase::Dosing,
        ..ReservoirTelemetry::UNAVAILABLE
    };
    let mut subject = TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
            reservoir: FixedReservoir(reservoir),
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        &sink,
        config(),
    );

    subject.sample().await;

    assert_eq!(sink.last().reservoir, reservoir);
}

#[test]
fn a_flood_republish_updates_each_zone_and_keeps_the_lock_visible_after_drying() {
    use crate::adapters::outbound::memory::VolatilePumpStore;
    use crate::application::PumpService;
    use crate::domain::flood::FloodProbes;
    use crate::domain::metrics::SystemMode;
    use crate::domain::pump::PumpSettings;
    use crate::ports::inbound::PumpControl;
    use std::sync::Arc;

    let sink = RecordingSink::default();
    let pumps = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        VolatilePumpStore::default(),
        FakeClock::synced_at_noon_in_june(),
        None,
    ));
    let collector = TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: Arc::clone(&pumps),
            reservoir: NoReservoir,
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        &sink,
        config(),
    );
    for (reservoir, sump) in [(true, false), (true, true), (false, true), (false, false)] {
        let probes = FloodProbes { reservoir, sump };
        pumps.observe_flood(probes);
        collector.republish();
        let data = sink.last();
        assert_eq!(data.leak_detected, reservoir || sump);
        assert_eq!(data.flood.probes, Some(probes));
        assert_eq!(data.system_mode, SystemMode::EmergencyLockout);
    }
    pumps.reset_flood().unwrap();
    collector.republish();
    assert_eq!(sink.last().system_mode, SystemMode::Normal);
}
