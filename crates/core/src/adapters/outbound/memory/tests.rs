use std::sync::Arc;

use tokio::time::Instant;

use super::{InMemoryTelemetryStore, VolatilePumpStore, VolatileReservoirStore};
use crate::domain::metrics::{ProbeVoltagesDto, SystemMode, TankTelemetryDto};
use crate::ports::inbound::{ProbeVoltageQuery, TelemetryQuery};
use crate::ports::outbound::{
    ProbeVoltageSink, PumpRecord, PumpStore, TelemetrySink, TemperatureSample,
    TemperatureSampleSink,
};

fn snapshot_with_uptime(uptime: u64) -> TankTelemetryDto {
    let mut s = TankTelemetryDto::empty();
    s.system_health.uptime_seconds = uptime;
    s
}

#[test]
fn serves_initial_snapshot_before_first_measurement() {
    let store = InMemoryTelemetryStore::new();
    let s = store.snapshot();
    assert_eq!(s.system_mode, SystemMode::Normal);
    assert_eq!(s.temperature.tank_celsius, None);
    assert!(!s.temperature.sensors_healthy);
}

#[test]
fn publish_replaces_the_snapshot() {
    let store = InMemoryTelemetryStore::new();
    store.publish(snapshot_with_uptime(42));
    assert_eq!(store.snapshot().system_health.uptime_seconds, 42);
}

#[tokio::test]
async fn subscriber_is_woken_by_publish() {
    let store = InMemoryTelemetryStore::new();
    let mut rx = store.subscribe();

    store.publish(snapshot_with_uptime(7));

    rx.changed()
        .await
        .expect("the channel must live as long as the store does");
    assert_eq!(rx.borrow().system_health.uptime_seconds, 7);
}

#[tokio::test]
async fn slow_subscriber_sees_only_the_latest_value() {
    let store = InMemoryTelemetryStore::new();
    let mut rx = store.subscribe();

    store.publish(snapshot_with_uptime(1));
    store.publish(snapshot_with_uptime(2));
    store.publish(snapshot_with_uptime(3));

    rx.changed().await.unwrap();
    assert_eq!(rx.borrow().system_health.uptime_seconds, 3);
}

#[tokio::test(start_paused = true)]
async fn temperature_samples_travel_on_their_own_channel() {
    let store = Arc::new(InMemoryTelemetryStore::new());
    let mut samples = store.subscribe_temperature();
    let snapshots = store.subscribe();
    assert_eq!(*samples.borrow(), None);

    let sample = TemperatureSample {
        tank_celsius: Some(25.5),
        sump_celsius: None,
        measured_at: tokio::time::Instant::now(),
    };
    store.publish_temperature(sample);

    samples
        .changed()
        .await
        .expect("the channel must live as long as the store does");
    assert_eq!(*samples.borrow(), Some(sample));
    assert!(!snapshots.has_changed().unwrap());
}

#[test]
fn arc_wrapper_forwards_publish() {
    let store = std::sync::Arc::new(InMemoryTelemetryStore::new());
    TelemetrySink::publish(&store, snapshot_with_uptime(5));
    assert_eq!(store.snapshot().system_health.uptime_seconds, 5);
}

#[test]
fn publishing_without_subscribers_does_not_panic() {
    let store = InMemoryTelemetryStore::new();
    store.publish(snapshot_with_uptime(1));
    assert_eq!(store.snapshot().system_health.uptime_seconds, 1);
}

mod water_level {
    use std::sync::Arc;

    use crate::adapters::outbound::memory::InMemoryWaterLevelStore;
    use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
    use crate::ports::outbound::{WaterLevelSink, WaterLevelSource};

    fn snapshot_with_wet(role: WaterLevelRole) -> WaterLevelSnapshot {
        let mut snapshot = WaterLevelSnapshot::dry();
        snapshot.sensors[role.index()].state = WaterLevelState::Wet;
        snapshot
    }

    #[test]
    fn water_level_subscribers_see_only_later_publications() {
        let store = InMemoryWaterLevelStore::new();
        let mut receiver = store.subscribe();

        assert!(!receiver.has_changed().expect("sender alive"));

        let wet = snapshot_with_wet(WaterLevelRole::ALL[0]);
        store.publish(wet.clone());

        assert!(receiver.has_changed().expect("sender alive"));
        assert_eq!(
            receiver
                .borrow_and_update()
                .as_ref()
                .map(|sample| &sample.levels),
            Some(&wet)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn a_published_sample_keeps_its_measurement_time() {
        let store = InMemoryWaterLevelStore::new();
        let receiver = store.subscribe();
        let measured = tokio::time::Instant::now();

        store.publish(snapshot_with_wet(WaterLevelRole::SumpPump));
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;

        let sample = receiver.borrow().clone().expect("published");
        assert_eq!(sample.measured_at, measured);
    }

    #[test]
    fn serves_no_levels_before_the_first_sample() {
        let store = InMemoryWaterLevelStore::new();

        assert_eq!(store.levels(), None);

        store.publish(WaterLevelSnapshot::dry());
        let levels = store.levels().expect("published").levels;
        for role in WaterLevelRole::ALL {
            assert_eq!(levels.get(role).state, WaterLevelState::Dry);
            assert!(!levels.get(role).faulty);
        }
    }

    #[test]
    fn publishing_replaces_the_snapshot() {
        let store = InMemoryWaterLevelStore::new();

        store.publish(snapshot_with_wet(WaterLevelRole::SumpPump));
        assert_eq!(
            store
                .levels()
                .unwrap()
                .levels
                .get(WaterLevelRole::SumpPump)
                .state,
            WaterLevelState::Wet
        );

        store.publish(snapshot_with_wet(WaterLevelRole::MainTank));
        let levels = store.levels().unwrap().levels;
        assert_eq!(
            levels.get(WaterLevelRole::SumpPump).state,
            WaterLevelState::Dry,
            "publish replaces the whole snapshot rather than merging"
        );
        assert_eq!(
            levels.get(WaterLevelRole::MainTank).state,
            WaterLevelState::Wet
        );
    }

    #[test]
    fn the_arc_forwards_reach_the_same_store() {
        let store = Arc::new(InMemoryWaterLevelStore::new());
        let writer = Arc::clone(&store);

        writer.publish(snapshot_with_wet(WaterLevelRole::SumpBiological));

        assert_eq!(
            store
                .levels()
                .unwrap()
                .levels
                .get(WaterLevelRole::SumpBiological)
                .state,
            WaterLevelState::Wet
        );
    }
}

#[test]
fn a_fresh_telemetry_store_reports_that_nothing_was_sampled_yet() {
    let store = InMemoryTelemetryStore::new();

    assert_eq!(
        store.probe_voltages(),
        ProbeVoltagesDto::NEVER_SAMPLED,
        "a zero timestamp is how the client tells that the device has not sampled"
    );
}

#[test]
fn the_telemetry_store_hands_back_the_last_published_voltages() {
    let store = InMemoryTelemetryStore::new();

    store.publish_volts(Some(1.52), Some(0.91), 1_750_000_000);

    assert_eq!(
        store.probe_voltages(),
        ProbeVoltagesDto {
            ph: Some(1.52),
            tds: Some(0.91),
            sampled_at_epoch_s: 1_750_000_000,
        }
    );
}

#[test]
fn a_missing_probe_is_published_as_absent_rather_than_zero() {
    let store = InMemoryTelemetryStore::new();

    store.publish_volts(None, Some(0.91), 1_750_000_000);

    let read = store.probe_voltages();

    assert_eq!(read.ph, None, "zero volts is a reading, absence is not");
    assert_eq!(read.tds, Some(0.91));
}

#[test]
fn publishing_voltages_replaces_the_previous_sample() {
    let store = InMemoryTelemetryStore::new();

    store.publish_volts(Some(1.50), None, 10);
    store.publish_volts(Some(1.51), None, 12);

    assert_eq!(store.probe_voltages().sampled_at_epoch_s, 12);
    assert_eq!(store.probe_voltages().ph, Some(1.51));
}

#[test]
fn voltages_published_through_an_arc_reach_the_store() {
    let store = Arc::new(InMemoryTelemetryStore::new());

    ProbeVoltageSink::publish_volts(&store, Some(1.49), Some(0.80), 7);

    assert_eq!(store.probe_voltages().ph, Some(1.49));
}

#[test]
fn the_volatile_pump_store_keeps_records_apart() {
    let store = VolatilePumpStore::default();

    assert_eq!(store.load(PumpRecord::Settings), None);

    store.store(PumpRecord::Settings, b"{}").expect("store");
    store.put(PumpRecord::Checkpoint, b"[1]".to_vec());

    assert_eq!(store.load(PumpRecord::Settings), Some(b"{}".to_vec()));
    assert_eq!(store.get(PumpRecord::Checkpoint), Some(b"[1]".to_vec()));
    assert_eq!(store.load(PumpRecord::Runtime), None);

    store.erase(PumpRecord::Settings).expect("erase");
    assert_eq!(store.load(PumpRecord::Settings), None);
}

#[test]
fn the_tds_channel_hands_out_the_latest_sample() {
    use crate::ports::outbound::{TdsSample, TdsSampleSink};

    let store = InMemoryTelemetryStore::new();
    let receiver = store.subscribe_tds();
    assert_eq!(*receiver.borrow(), None);

    let first = TdsSample {
        ppm: Some(150.0),
        measured_at: Instant::now(),
    };
    let second = TdsSample {
        ppm: None,
        measured_at: Instant::now(),
    };
    store.publish_tds(first);
    store.publish_tds(second);
    assert_eq!(*receiver.borrow(), Some(second));
}

#[test]
fn the_volatile_reservoir_store_keeps_records_apart() {
    use crate::ports::outbound::{ReservoirRecord, ReservoirStore};

    let store = VolatileReservoirStore::default();
    store.store(ReservoirRecord::Fault, b"x").expect("stored");
    assert_eq!(
        store.load(ReservoirRecord::Fault).unwrap(),
        Some(b"x".to_vec())
    );
    assert_eq!(store.load(ReservoirRecord::Running).unwrap(), None);
    store.erase(ReservoirRecord::Fault).expect("erased");
    assert_eq!(store.load(ReservoirRecord::Fault).unwrap(), None);
}
