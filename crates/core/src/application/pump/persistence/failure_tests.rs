use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::application::pump::bench::*;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Checkpoint, Operation, OperationKind, OperationState, OperationStatus, PumpId, SkipReason,
};
use crate::ports::inbound::{CancelError, PumpControl, StartError};
use crate::ports::outbound::PumpRecord;

#[test]
fn an_api_start_that_cannot_be_recorded_switches_nothing_on() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.store.refuse(true);

    assert!(matches!(
        bench.service.start_water_change(PLAN),
        Err(StartError::NotPersisted(_))
    ));
    assert!(matches!(
        bench.service.start_top_off(60),
        Err(StartError::NotPersisted(_))
    ));
    bench.run(3, &healthy);

    assert_eq!(bench.service.status().operation, OperationStatus::IDLE);
    assert!(!bench.outputs.switched_on(PumpId::Drain));
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
}

#[test]
fn a_water_change_that_cannot_be_recorded_leaves_the_running_top_off_alone() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_top_off(60).expect("idle");
    bench.run(1, &healthy);
    bench.store.refuse(true);

    assert!(matches!(
        bench.service.start_water_change(PLAN),
        Err(StartError::NotPersisted(_))
    ));
    bench.run(1, &healthy);

    let status = bench.service.status();
    assert_eq!(status.operation.kind, Some(OperationKind::TopOff));
    assert_eq!(status.last_top_off, None);
    assert!(bench.outputs.pin(PumpId::Makeup));
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_scheduled_start_that_cannot_be_recorded_is_skipped_for_storage() {
    let mut bench = Bench::new();
    let healthy = healthy();
    schedule_top_off_on(&bench.service);
    bench.tick(&healthy);
    bench.store.refuse(true);

    bench.clock.set_civil(Some(sunday_ten_am()));
    bench.run(3, &healthy);

    let skipped = bench.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Storage);
    assert_eq!(skipped.blocked_by, None);
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
}

fn seed_flaky_checkpoint(store: &FlakyStore, operation: Operation, checkpoint_at: Option<u64>) {
    store.inner.put(
        PumpRecord::Checkpoint,
        serde_json::to_vec(&Checkpoint {
            operation,
            checkpoint_at,
        })
        .expect("encode"),
    );
}

#[test]
fn an_automatic_resume_that_cannot_be_recorded_suspends_without_switching() {
    let store = Arc::new(FlakyStore::default());
    seed_flaky_checkpoint(&store, interrupted_drain(EPOCH - 200), Some(EPOCH - 100));
    store.refuse(true);
    let mut bench = Bench::on(store, TestClock::synced(EPOCH));
    let healthy = healthy();

    bench.tick(&healthy);
    bench.run(3, &healthy);

    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Suspended
    );
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn an_api_resume_that_cannot_be_recorded_stays_suspended() {
    let store = Arc::new(FlakyStore::default());
    seed_flaky_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut bench = Bench::on(store, TestClock::synced(EPOCH));
    let healthy = healthy();
    bench.tick(&healthy);
    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Suspended
    );

    bench.store.refuse(true);
    assert!(matches!(
        bench.service.resume(OperationKind::WaterChange),
        Err(StartError::NotPersisted(_))
    ));
    bench.run(3, &healthy);

    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Suspended
    );
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_cancel_that_cannot_be_recorded_stops_the_drain_and_retries_until_it_lands() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_water_change(PLAN).expect("idle");
    bench.run(1, &healthy);
    assert!(bench.outputs.pin(PumpId::Drain));

    bench.store.refuse(true);
    assert!(matches!(
        bench.service.cancel(OperationKind::WaterChange),
        Err(CancelError::NotPersisted(_))
    ));
    bench.run(1, &healthy);
    assert!(!bench.outputs.pin(PumpId::Drain));
    assert!(bench.service.status().invalidation_pending);
    assert!(bench.store.holds_checkpoint());

    let tried = bench.store.invalidations.load(Ordering::Relaxed);
    bench.run(3, &healthy);
    assert!(bench.store.invalidations.load(Ordering::Relaxed) >= tried + 3);
    assert!(bench.service.status().invalidation_pending);

    bench.store.refuse(false);
    bench.run(1, &healthy);
    assert!(!bench.service.status().invalidation_pending);
    assert!(!bench.store.holds_checkpoint());

    let mut rebooted = bench.reboot();
    rebooted.tick(&healthy);
    rebooted.run(5, &healthy);
    assert_eq!(rebooted.service.status().operation, OperationStatus::IDLE);
    assert!(!rebooted.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_new_operation_supersedes_a_pending_invalidation() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_top_off(60).expect("idle");
    bench.run(1, &healthy);
    bench.store.refuse(true);
    assert!(bench.service.cancel(OperationKind::TopOff).is_err());
    bench.run(1, &healthy);

    bench.store.refuse(false);
    bench.service.start_top_off(60).expect("idle");
    assert!(!bench.service.status().invalidation_pending);
    bench.run(1, &healthy);
    assert!(bench.store.holds_checkpoint());
    assert_eq!(
        bench.service.status().operation.kind,
        Some(OperationKind::TopOff)
    );
}
