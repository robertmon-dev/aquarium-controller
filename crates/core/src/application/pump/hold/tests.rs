use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::application::pump::{PumpController, PumpService};
use crate::domain::pump::{
    Blocker, LastOperations, OperationKind, OperationResult, OperationState, PumpId, PumpSettings,
};
use crate::domain::water::WaterLevelSnapshot;
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::{Clock, PumpRecord, PumpStatusSource, PumpStore};

fn unplugged() -> WaterLevelSnapshot {
    WaterLevelSnapshot::dry()
}

#[test]
fn an_unplugged_harness_mid_drain_holds_the_return_off() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));

    rig.run(1, Some(&unplugged()));
    let status = rig.service.status();
    assert!(status.level_sensors_suspect);
    assert!(status.return_held_after_drain);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::LevelsUnavailableAfterDrain)
    );
    assert_eq!(status.channels.uv.blocked_by, Some(Blocker::ReturnOff));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(3600, Some(&unplugged()));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&running_sump()));
    assert!(!rig.service.status().return_held_after_drain);
}

fn abort_a_water_change(rig: &mut Rig) {
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
}

#[test]
fn an_unplugged_harness_within_thirty_minutes_of_an_aborted_drain_holds_the_return() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(29 * 60, Some(&running_sump()));

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().return_held_after_drain);
    assert!(!rig.pins()[PumpId::Return.index()]);
}

#[test]
fn an_unplugged_harness_long_after_an_aborted_drain_keeps_the_return() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(31 * 60, Some(&running_sump()));

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().level_sensors_suspect);
    assert!(!rig.service.status().return_held_after_drain);
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn an_unplugged_harness_after_a_water_change_that_filled_the_sump_keeps_the_return() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(6, Some(&healthy()));
    rig.run(3, Some(&full_sump()));
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::SumpFull)
    );

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().level_sensors_suspect);
    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);
}

#[test]
fn an_unplugged_harness_while_a_water_change_is_suspended_holds_the_return() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.run(5, Some(&running_sump()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().return_held_after_drain);
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn pump_telemetry_carries_the_storage_and_hold_flags() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));
    rig.run(1, Some(&unplugged()));

    let telemetry = rig.service.pump_telemetry();
    assert!(telemetry.return_held_after_drain);
    assert!(!telemetry.invalidation_pending);
}

#[test]
fn levels_lost_mid_drain_hold_the_return_until_fresh_data_returns() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));

    rig.run(1, None);
    let status = rig.service.status();
    assert!(!status.level_sensors_suspect);
    assert!(status.return_held_after_drain);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::LevelsUnavailableAfterDrain)
    );
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(3600, None);
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&running_sump()));
    assert!(!rig.service.status().return_held_after_drain);
}

#[test]
fn levels_lost_within_thirty_minutes_of_an_aborted_drain_hold_the_return() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(29 * 60, Some(&running_sump()));

    rig.run(1, None);

    assert!(rig.service.status().return_held_after_drain);
    assert!(!rig.pins()[PumpId::Return.index()]);
}

#[test]
fn levels_lost_at_rest_keep_the_return_running() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));

    rig.run(5, None);

    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);

    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(31 * 60, Some(&running_sump()));
    rig.run(1, None);
    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);
}

#[test]
fn a_finished_water_change_records_when_it_ended() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.finished_at, Some(EPOCH + 6));
    let stored: LastOperations = decode(&rig.store, PumpRecord::LastOperations).expect("stored");
    assert_eq!(stored.water_change, Some(record));
}

#[test]
fn the_blind_return_hold_window_survives_a_restart() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(10 * 60, Some(&running_sump()));

    let mut restarted = Rig::build(Arc::clone(&rig.store), Arc::clone(&rig.clock), None);
    restarted.run(1, None);
    assert!(restarted.service.status().return_held_after_drain);
    assert!(!restarted.pins()[PumpId::Return.index()]);

    restarted.run(1, Some(&running_sump()));
    assert!(!restarted.service.status().return_held_after_drain);

    rig.clock.advance(25 * 60);
    let mut later = Rig::build(Arc::clone(&rig.store), Arc::clone(&rig.clock), None);
    later.run(1, None);
    assert!(!later.service.status().return_held_after_drain);
    assert!(later.pins()[PumpId::Return.index()]);
}

#[test]
fn regression_a_recent_aborted_drain_keeps_the_return_off_before_clock_sync() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    let record = rig
        .service
        .status()
        .last_water_change
        .expect("aborted drain");
    assert!(record.finished_at.is_some());

    let mut restarted = Rig::build(rig.store, TestClock::unsynced(), None);
    restarted.run(1, None);
    assert!(
        !restarted.pins()[PumpId::Return.index()],
        "the return switched on before the clock or level readings recovered"
    );
    assert!(restarted.service.status().return_held_after_drain);
}

#[test]
fn regression_an_active_return_hold_survives_reboot_after_thirty_minutes() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(31 * 60, None);
    assert!(rig.service.status().return_held_after_drain);
    assert!(!rig.pins()[PumpId::Return.index()]);

    let epoch = rig.clock.epoch_seconds().expect("synchronized clock");
    let mut restarted = Rig::build(rig.store, TestClock::synced(epoch), None);
    restarted.run(1, None);
    assert!(
        !restarted.pins()[PumpId::Return.index()],
        "reboot released the hold while the same sensor outage continued"
    );
    assert!(restarted.service.status().return_held_after_drain);

    restarted.run(31, Some(&running_sump()));
    assert!(!restarted.service.status().return_held_after_drain);
    assert!(restarted.pins()[PumpId::Return.index()]);
}

#[test]
fn the_return_hold_is_stored_when_set_and_when_released() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);

    rig.run(1, None);
    assert_eq!(
        decode::<bool>(&rig.store, PumpRecord::ReturnHold),
        Some(true)
    );

    rig.run(1, Some(&running_sump()));
    assert_eq!(
        decode::<bool>(&rig.store, PumpRecord::ReturnHold),
        Some(false)
    );
}

#[test]
fn storage_without_a_return_hold_starts_unheld() {
    let mut rig = Rig::new();

    rig.run(5, None);

    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);
    assert_eq!(decode::<bool>(&rig.store, PumpRecord::ReturnHold), None);
}

#[derive(Default)]
struct HoldRefusingStore {
    inner: VolatilePumpStore,
    refuse: std::sync::atomic::AtomicBool,
}

impl PumpStore for HoldRefusingStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.inner.load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        if record == PumpRecord::ReturnHold && self.refuse.load(Ordering::Relaxed) {
            return Err("flash refused the write");
        }
        self.inner.store(record, bytes)
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        self.inner.erase(record)
    }
}

#[test]
fn a_refused_return_hold_write_is_retried() {
    let store = Arc::new(HoldRefusingStore::default());
    let clock = TestClock::synced(EPOCH);
    let service = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        Arc::clone(&store),
        Arc::clone(&clock),
        None,
    ));
    let outputs = RecordingOutputs::default();
    let mut controller = PumpController::new(outputs.clone(), Arc::clone(&service), timing());
    let mut now = Instant::now();
    let mut run = |seconds: u64, levels: Option<&WaterLevelSnapshot>| {
        for _ in 0..seconds {
            now += Duration::from_secs(1);
            clock.advance(1);
            controller.tick(levels, now);
        }
    };

    run(5, Some(&running_sump()));
    service.start_water_change(PLAN).expect("idle");
    run(1, Some(&running_sump()));
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    store.refuse.store(true, Ordering::Relaxed);
    run(3, None);

    assert!(service.status().return_held_after_drain);
    assert!(!outputs.state()[PumpId::Return.index()]);
    assert!(store.inner.get(PumpRecord::ReturnHold).is_none());

    store.refuse.store(false, Ordering::Relaxed);
    run(1, None);

    assert_eq!(
        store.inner.get(PumpRecord::ReturnHold).as_deref(),
        Some(&b"true"[..])
    );
}
