use std::sync::Arc;

use tokio::sync::mpsc;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Blocker, ChannelFault, Checkpoint, OperationResult, OperationState, OperationStatus, PumpId,
    WaterChangePlan,
};
use crate::domain::water::WaterLevelRole;
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::PumpRecord;

#[test]
fn a_flooded_sump_ends_a_running_top_off_with_an_overflow() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let flooded = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpMechanical,
    ]);

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(2, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.run(1, Some(&flooded));
    assert!(!rig.pins()[2]);
    let status = rig.service.status();
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpHigh));
    assert!(status.channels.return_pump.on);
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::Overflow)
    );
}

#[test]
fn a_top_off_runs_makeup_then_reports_the_result() {
    let (events, mut received) = mpsc::unbounded_channel();
    let mut rig = Rig::build(
        Arc::new(VolatilePumpStore::default()),
        TestClock::synced(EPOCH),
        Some(events),
    );
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_top_off(3).expect("idle");
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, true, true, false]);

    rig.run(3, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);

    let status = rig.service.status();
    assert_eq!(status.operation, OperationStatus::IDLE);
    let record = status.last_top_off.expect("recorded");
    assert_eq!(
        (record.result, record.refill_ran_s),
        (OperationResult::Completed, 3)
    );

    let event = received.try_recv().expect("event");
    assert_eq!(event.record, record);
    assert_eq!(event.finished_at, Some(EPOCH + 3));
}

#[test]
fn a_water_change_drains_settles_refills_and_stops_on_overflow() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let overflow = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::MainTank,
    ]);

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 2,
            settle_s: 2,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);
    let status = rig.service.status();
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
    assert_eq!(status.channels.uv.blocked_by, Some(Blocker::ReturnOff));

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        rig.service.status().channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, true, false]);
    assert_eq!(rig.service.status().channels.return_pump.blocked_by, None);

    rig.run(1, Some(&overflow));
    assert_eq!(rig.pins(), [true, true, false, false]);

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::Overflow);
    assert_eq!((record.drain_ran_s, record.settle_ran_s), (2, 2));
}

#[test]
fn losing_the_level_sensors_aborts_a_water_change() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&healthy));
    rig.run(1, None);

    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::Aborted)
    );
}

#[test]
fn operation_time_does_not_accrue_while_the_drain_output_fails() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Drain);
    rig.tick(Some(&healthy));
    rig.run(10, Some(&healthy));

    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(status.operation.drain_ran_s, 0);
    assert_eq!(status.channels.drain.fault, Some(ChannelFault::OutputError));
}

#[test]
fn a_dry_sump_does_not_end_the_drain_and_the_refill_waits_out_the_lockout() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]);

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);

    rig.run(3, Some(&low));
    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Settling);
    assert_eq!(status.operation.drain_ran_s, 3);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );

    rig.run(2, Some(&low));
    assert_eq!(rig.pins(), [false, false, true, false]);
    assert_eq!(
        rig.service.status().channels.return_pump.blocked_by,
        Some(Blocker::SumpLow)
    );

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, true, false]);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::Completed)
    );
}

#[test]
fn a_water_change_refill_into_a_full_sump_waits_for_the_return_pump() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let full = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpReturnLevel,
    ]);

    rig.tick(Some(&full));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&full));
    rig.run(5, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(30, Some(&full));
    let status = rig.service.status();
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(status.operation.state, OperationState::Refilling);
    assert_eq!(status.operation.refill_ran_s, 0);
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpFull));

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, true, false]);

    rig.run(1, Some(&full));
    assert_eq!(rig.pins(), [true, true, false, false]);
    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::SumpFull);
    assert_eq!(record.refill_ran_s, 2);
}

#[test]
fn an_empty_reservoir_stops_the_refill() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.run(1, Some(&empty_reservoir));
    assert!(!rig.pins()[2]);
    assert_eq!(
        rig.service
            .status()
            .last_top_off
            .map(|record| record.result),
        Some(OperationResult::ReservoirEmpty)
    );
}

#[test]
fn the_configured_refill_stop_sensor_ends_a_top_off() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let full = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::SumpBiological,
    ]);
    set_refill_stop(&rig.service, WaterLevelRole::SumpBiological);

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.run(1, Some(&full));
    assert!(!rig.pins()[2]);

    let status = rig.service.status();
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpFull));
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::SumpFull)
    );
}

#[test]
fn a_running_refill_stalls_while_the_return_pump_is_forced_off() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));
    rig.service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");
    rig.run(5, Some(&healthy));

    let status = rig.service.status();
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::ReturnOff));
    assert_eq!(status.operation.refill_ran_s, 1);
}

#[test]
fn a_refill_stuck_on_a_wet_stop_sensor_times_out() {
    let mut rig = Rig::new();
    let full = full_sump();

    rig.tick(Some(&full));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&full));
    rig.run(5, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(304, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );
    let checkpoint: Checkpoint = decode(&rig.store, PumpRecord::Checkpoint).expect("stored");
    assert!(
        checkpoint.operation.refill_wall_ms >= 290_000,
        "{}",
        checkpoint.operation.refill_wall_ms
    );

    rig.run(1, Some(&full));
    let status = rig.service.status();
    assert_eq!(status.operation, OperationStatus::IDLE);
    let record = status.last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::RefillTimeout);
    assert_eq!(record.refill_ran_s, 0);
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        decode::<Option<Checkpoint>>(&rig.store, PumpRecord::Checkpoint).flatten(),
        None
    );
}
