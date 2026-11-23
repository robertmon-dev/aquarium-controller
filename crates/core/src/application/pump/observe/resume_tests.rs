use std::sync::Arc;
use std::time::Duration;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Blocker, LevelConditions, Operation, OperationKind, OperationPlan, OperationResult,
    OperationState, Phase, PumpId, Supervision, WaterChangePlan,
};
use crate::domain::water::WaterLevelRole;
use crate::ports::inbound::{PumpControl, StartError};
use crate::ports::outbound::PumpRecord;

#[test]
fn an_interrupted_water_change_resumes_after_a_short_outage() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 200), Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);
    let operation = rig.service.status().operation;
    assert_eq!(operation.state, OperationState::Draining);
    assert_eq!(operation.phase_remaining_s, Some(6));

    rig.run(20, Some(&healthy));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert!(record.resumed);
    assert_eq!(record.drain_ran_s, 10);
    assert_eq!(record.result, OperationResult::Completed);
}

#[test]
fn the_resume_gap_is_measured_once_at_the_first_time_sync() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 400), Some(EPOCH - 300));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(None);
    rig.run(310, None);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );

    rig.run(1, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[test]
fn resuming_gives_up_when_levels_stay_unavailable_for_the_whole_window() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 400), Some(EPOCH - 300));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(None);
    rig.run(599, None);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );

    rig.run(1, None);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );
}

#[test]
fn a_long_outage_suspends_until_resumed_over_the_api() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    assert_eq!(
        rig.service.resume(OperationKind::TopOff),
        Err(StartError::Conflict)
    );
    rig.service
        .resume(OperationKind::WaterChange)
        .expect("suspended");

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);
    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
}

#[test]
fn resuming_a_water_change_does_not_need_a_ready_reservoir() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let reservoir_low = snapshot(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]);

    rig.tick(Some(&reservoir_low));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    assert_eq!(rig.service.resume(OperationKind::WaterChange), Ok(()));
    rig.tick(Some(&reservoir_low));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[test]
fn cancelling_a_suspended_operation_records_the_reboot() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(Arc::clone(&store), TestClock::synced(EPOCH), None);

    rig.tick(Some(&healthy()));
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");

    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::InterruptedByReboot)
    );
    assert_eq!(store.get(PumpRecord::Checkpoint), None);
}

#[test]
fn resuming_waits_for_time_and_gives_up_after_the_window() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 20), Some(EPOCH - 10));
    let mut rig = Rig::build(store, TestClock::unsynced(), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.run(599, Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );
    assert!(!rig.pins()[3]);

    rig.run(1, Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );
}

#[test]
fn a_resumed_settling_pause_keeps_the_return_pump_off() {
    let mut operation = Operation::start(
        OperationPlan::water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 30,
            refill_s: 10,
        }),
        Some(EPOCH - 200),
    );
    operation.advance(10_000, 10_000, Some(HEALTHY));
    operation.advance(5000, 5000, Some(HEALTHY));
    assert_eq!(operation.phase, Phase::Settling);
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, operation, Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(Some(&healthy()));

    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Settling);
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
}

#[test]
fn an_automatic_resume_does_not_credit_the_elapsed_gap() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 200), Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(None);
    rig.now += Duration::from_secs(300);
    rig.tick(Some(&healthy()));

    let operation = rig.service.status().operation;
    assert_eq!(operation.state, OperationState::Draining);
    assert_eq!(operation.drain_ran_s, 4);
}

#[test]
fn an_api_resume_does_not_credit_the_elapsed_gap() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service
        .resume(OperationKind::WaterChange)
        .expect("suspended");
    rig.now += Duration::from_secs(300);
    rig.tick(Some(&healthy));

    let operation = rig.service.status().operation;
    assert_eq!(operation.state, OperationState::Draining);
    assert_eq!(operation.drain_ran_s, 4);
}

#[test]
fn an_operation_resumes_automatically_only_once() {
    let store = Arc::new(VolatilePumpStore::default());
    let mut operation = interrupted_drain(EPOCH - 200);
    operation.resume();
    seed_checkpoint(&store, operation, Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    rig.service
        .resume(OperationKind::WaterChange)
        .expect("suspended");
    rig.tick(Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[test]
fn a_resumed_refill_continues_its_wall_time_budget() {
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    let mut operation = Operation::start(
        OperationPlan::water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 60,
        }),
        Some(EPOCH - 400),
    );
    operation.advance(10_000, 10_000, Some(HEALTHY));
    operation.advance(0, 0, Some(HEALTHY));
    assert_eq!(operation.phase, Phase::Refilling);
    operation.advance(0, 350_000, Some(full));
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, operation, Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let full = full_sump();

    rig.tick(None);
    rig.now += Duration::from_secs(300);
    rig.tick(Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(10, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(1, Some(&full));
    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::RefillTimeout);
    assert!(record.resumed);
}

#[test]
fn a_suspended_operation_does_not_resume_without_supervision() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.run(5, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    rig.service.set_supervision(Supervision::Unavailable);
    assert_eq!(
        rig.service.resume(OperationKind::WaterChange),
        Err(StartError::Blocked(Blocker::SupervisionUnavailable))
    );

    rig.service.set_supervision(Supervision::Active);
    assert_eq!(rig.service.resume(OperationKind::WaterChange), Ok(()));
}

#[test]
fn an_interrupted_drain_waits_for_supervision_before_resuming_on_its_own() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 20), Some(EPOCH - 10));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.service.set_supervision(Supervision::Unavailable);

    rig.run(3, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );
    assert!(!rig.pins()[PumpId::Drain.index()]);

    rig.service.set_supervision(Supervision::Active);
    rig.run(2, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}
