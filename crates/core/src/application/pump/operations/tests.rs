use std::sync::Arc;

use tokio::sync::mpsc;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Blocker, Checkpoint, LevelConditions, OperationKind, OperationResult, OperationState,
    OperationStatus, PumpId, PumpSettingsPatch, WaterChangePlan,
};
use crate::domain::water::WaterLevelRole;
use crate::ports::inbound::{PumpControl, StartError};
use crate::ports::outbound::PumpRecord;

#[test]
fn operations_need_levels_and_respect_blockers() {
    let (service, _store) = service_and_store();

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::LevelsUnavailable))
    );
    assert_eq!(
        service.start_top_off(0),
        Err(StartError::Invalid("a top-off lasts 1 to 600 s"))
    );
    assert_eq!(
        service.start_water_change(WaterChangePlan { drain_s: 0, ..PLAN }),
        Err(StartError::Invalid("drain_s must be between 1 and 900"))
    );

    observe(
        &service,
        Some(LevelConditions {
            reservoir_empty: true,
            ..HEALTHY
        }),
    );
    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::ReservoirEmpty))
    );

    observe(
        &service,
        Some(LevelConditions {
            tank_overflow: true,
            ..HEALTHY
        }),
    );
    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::TankOverflow))
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::TankOverflow))
    );

    observe(&service, Some(HEALTHY));
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_high_sump_refuses_both_operations() {
    let (service, _store) = service_and_store();
    observe(
        &service,
        Some(LevelConditions {
            sump_high: true,
            ..HEALTHY
        }),
    );

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::SumpHigh))
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::SumpHigh))
    );
}

#[test]
fn a_water_change_needs_a_ready_reservoir_but_a_top_off_does_not() {
    let (service, _store) = service_and_store();
    observe(
        &service,
        Some(LevelConditions {
            reservoir_ready: false,
            ..HEALTHY
        }),
    );

    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::ReservoirLow))
    );
    assert_eq!(service.start_top_off(60), Ok(()));
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::ReservoirLow))
    );
    assert_eq!(
        service.status().operation.kind,
        Some(OperationKind::TopOff),
        "a refused water change must not abort the running top-off"
    );
    service.cancel(OperationKind::TopOff).expect("cancelled");

    observe(&service, Some(HEALTHY));
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_water_change_preempts_a_top_off_but_not_the_other_way_round() {
    let (service, store) = service_and_store();
    observe(&service, Some(HEALTHY));

    service.start_top_off(60).expect("idle");
    assert_eq!(service.start_top_off(60), Err(StartError::Conflict));

    service.start_water_change(PLAN).expect("preempts");
    let status = service.status();
    assert_eq!(status.operation.kind, Some(OperationKind::WaterChange));
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::Aborted)
    );

    assert_eq!(service.start_top_off(60), Err(StartError::Conflict));
    assert_eq!(service.start_water_change(PLAN), Err(StartError::Conflict));

    let checkpoint: Checkpoint = decode(&store, PumpRecord::Checkpoint).expect("checkpoint");
    assert_eq!(checkpoint.operation.plan.kind, OperationKind::WaterChange);
    assert_eq!(checkpoint.checkpoint_at, Some(EPOCH));
}

#[test]
fn cancelling_records_an_abort_and_erases_the_checkpoint() {
    let (service, store) = service_and_store();
    observe(&service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");

    service.cancel(OperationKind::TopOff).expect("cancelled");
    assert_eq!(
        service.status().operation.kind,
        Some(OperationKind::WaterChange)
    );

    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    let status = service.status();
    assert_eq!(status.operation, OperationStatus::IDLE);
    assert_eq!(
        status.last_water_change.map(|record| record.result),
        Some(OperationResult::Aborted)
    );
    assert_eq!(store.get(PumpRecord::Checkpoint), None);
    assert!(store.get(PumpRecord::LastOperations).is_some());
}

#[test]
fn a_top_off_does_not_start_into_a_full_sump() {
    let (service, _store) = service_and_store();
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    observe(&service, Some(full));

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::SumpFull))
    );
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn refills_do_not_start_while_the_return_pump_is_forced_off() {
    let (service, _store) = service_and_store();
    observe(&service, Some(HEALTHY));
    service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::ReturnOff))
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::ReturnOff))
    );

    service.clear_override(PumpId::Return).expect("overridable");
    assert_eq!(service.start_top_off(60), Ok(()));
}

#[test]
fn a_water_change_draining_more_than_the_ready_reservoir_is_refused() {
    let (service, _store) = service_and_store();
    service
        .apply_settings(reservoir_patch(1500, Some(50)))
        .expect("valid");
    observe(&service, Some(HEALTHY));

    assert_eq!(
        service.start_water_change(WaterChangePlan {
            drain_s: 121,
            ..PLAN
        }),
        Err(StartError::Invalid(
            "the planned drain exceeds the water the reservoir holds"
        ))
    );
    assert_eq!(service.status().operation, OperationStatus::IDLE);
    assert_eq!(service.status().last_water_change, None);

    assert_eq!(
        service.start_water_change(WaterChangePlan {
            drain_s: 120,
            ..PLAN
        }),
        Ok(())
    );
}

fn configure_flows(service: &Service) {
    service
        .apply_settings(PumpSettingsPatch {
            flow_lph: Some(crate::domain::pump::FlowRates {
                makeup: Some(600),
                drain: Some(1500),
            }),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

#[test]
fn a_water_change_that_refilled_too_little_is_recorded_incomplete() {
    let (sender, mut events) = mpsc::unbounded_channel();
    let mut rig = Rig::build(
        Arc::new(VolatilePumpStore::default()),
        TestClock::synced(EPOCH),
        Some(sender),
    );
    configure_flows(&rig.service);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 3,
        })
        .expect("idle");
    rig.tick(Some(&healthy));
    rig.run(16, Some(&healthy));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.drain_ran_s, 10);
    assert_eq!(record.refill_ran_s, 3);
    assert_eq!(record.result, OperationResult::Incomplete);
    let stored: crate::domain::pump::LastOperations =
        decode(&rig.store, PumpRecord::LastOperations).expect("stored");
    assert_eq!(
        stored.water_change.map(|record| record.result),
        Some(OperationResult::Incomplete)
    );
    let event = events.try_recv().expect("an event");
    assert_eq!(event.record.result, OperationResult::Incomplete);
}

#[test]
fn a_water_change_that_stopped_on_the_full_sump_is_not_graded() {
    let mut rig = Rig::new();
    configure_flows(&rig.service);
    let healthy = healthy();
    let full = full_sump();

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));
    rig.run(11, Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(2, Some(&healthy));
    rig.run(1, Some(&full));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.refill_ran_s, 3);
    assert_eq!(record.result, OperationResult::SumpFull);
}

#[test]
fn a_water_change_refill_that_runs_out_its_time_with_the_sump_low_is_incomplete() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]);

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&healthy));
    rig.run(5, Some(&healthy));
    rig.run(5, Some(&low));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.refill_ran_s, PLAN.refill_s);
    assert_eq!(record.result, OperationResult::Incomplete);
}

#[test]
fn a_top_off_is_never_graded_incomplete() {
    let mut rig = Rig::new();
    configure_flows(&rig.service);
    let low = snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]);

    rig.tick(Some(&low));
    rig.service.start_top_off(3).expect("idle");
    rig.tick(Some(&low));
    rig.run(4, Some(&low));

    assert_eq!(
        rig.service
            .status()
            .last_top_off
            .map(|record| record.result),
        Some(OperationResult::Completed)
    );
}
