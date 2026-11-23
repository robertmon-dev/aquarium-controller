use crate::application::pump::bench::*;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Blocker, ChannelFault, OperationResult, OperationState, PumpId, Supervision, WaterChangePlan,
};
use crate::ports::inbound::PumpControl;

const QUICK_DRAIN: WaterChangePlan = WaterChangePlan {
    drain_s: 1,
    settle_s: 0,
    refill_s: 4,
};

#[test]
fn each_water_mover_needs_a_successful_off_for_its_opposite_even_from_idle() {
    for (starting, opposite) in [
        (PumpId::Drain, PumpId::Makeup),
        (PumpId::Makeup, PumpId::Drain),
    ] {
        let mut bench = Bench::new();
        let levels = healthy();
        bench.tick(&levels);
        bench.outputs.refuse_off(Some(opposite));
        if starting == PumpId::Drain {
            bench.service.start_water_change(PLAN).unwrap();
        } else {
            bench.service.start_top_off(60).unwrap();
        }
        bench.tick(&levels);
        assert!(!bench.outputs.switched_on(starting));
        assert!(bench.outputs.failed_offs(opposite) > 0);

        bench.outputs.refuse_off(None);
        bench.run(1, &levels);
        assert!(bench.outputs.pin(starting));
    }
}

#[test]
fn regression_a_stalled_drain_releases_circulation() {
    let mut bench = Bench::new();
    let levels = healthy();
    bench.service.set_supervision(Supervision::Active);
    bench.tick(&levels);
    assert!(bench.outputs.pin(PumpId::Return));

    bench.service.start_water_change(PLAN).expect("accepted");
    bench.outputs.refuse_off(Some(PumpId::Makeup));
    bench.run(60 * 60, &levels);

    assert!(bench.outputs.failed_offs(PumpId::Makeup) > 0);
    assert!(!bench.outputs.switched_on(PumpId::Drain));
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
    let status = bench.service.status();
    assert_eq!(
        status.operation.state,
        OperationState::Idle,
        "an hour elapsed without the drain ever switching on"
    );
    let record = status.last_water_change.expect("stalled drain recorded");
    assert_eq!(record.drain_ran_s, 0);
    assert!(!matches!(
        record.result,
        OperationResult::Completed | OperationResult::SumpFull | OperationResult::Overflow
    ));
    assert!(bench.outputs.pin(PumpId::Return));
    assert!(bench.outputs.pin(PumpId::Uv));
}

#[test]
fn the_drain_waits_for_a_successful_return_off() {
    let mut bench = Bench::new();
    let levels = healthy();
    bench.service.set_supervision(Supervision::Active);
    bench.tick(&levels);
    assert!(bench.outputs.pin(PumpId::Return));

    bench.outputs.refuse_off(Some(PumpId::Return));
    bench.service.start_water_change(PLAN).expect("accepted");
    bench.run(5, &levels);

    assert!(!bench.outputs.switched_on(PumpId::Drain));
    let status = bench.service.status();
    assert_eq!(status.channels.drain.blocked_by, Some(Blocker::Interlock));

    bench.outputs.refuse_off(None);
    bench.run(2, &levels);
    assert!(!bench.outputs.pin(PumpId::Return));
    assert!(bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_drain_that_fails_to_switch_off_keeps_the_makeup_off() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_water_change(QUICK_DRAIN).expect("idle");
    bench.tick(&healthy);
    assert!(bench.outputs.pin(PumpId::Drain));

    bench.outputs.refuse_off(Some(PumpId::Drain));
    bench.run(5, &healthy);

    let status = bench.service.status();
    assert_eq!(status.operation.state, OperationState::Refilling);
    assert_eq!(status.channels.drain.fault, Some(ChannelFault::OutputError));
    assert!(status.channels.drain.on, "the drain may still be running");
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::Interlock));
    assert!(bench.outputs.pin(PumpId::Drain));
    assert!(!bench.outputs.pin(PumpId::Makeup));
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
    assert!(bench.outputs.failed_offs(PumpId::Drain) >= 4);

    bench.outputs.refuse_off(None);
    bench.run(1, &healthy);
    assert!(!bench.outputs.pin(PumpId::Drain));
    assert!(bench.outputs.pin(PumpId::Makeup));
    assert_eq!(bench.service.status().channels.drain.fault, None);
}

#[test]
fn a_water_change_does_not_drain_while_the_top_off_makeup_may_still_run() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_top_off(60).expect("idle");
    bench.run(1, &healthy);
    assert!(bench.outputs.pin(PumpId::Makeup));

    bench.outputs.refuse_off(Some(PumpId::Makeup));
    bench.service.start_water_change(PLAN).expect("preempts");
    bench.run(3, &healthy);

    let status = bench.service.status();
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(status.operation.drain_ran_s, 0);
    assert_eq!(
        status.channels.makeup.fault,
        Some(ChannelFault::OutputError)
    );
    assert_eq!(status.channels.drain.blocked_by, Some(Blocker::Interlock));
    assert!(bench.outputs.pin(PumpId::Makeup));
    assert!(!bench.outputs.pin(PumpId::Drain));
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn regression_losing_supervision_while_settling_keeps_makeup_off() {
    let mut bench = Bench::new();
    let levels = healthy();
    bench.service.set_supervision(Supervision::Active);
    bench.tick(&levels);
    bench.service.start_water_change(PLAN).expect("accepted");
    bench.tick(&levels);
    bench.run(u64::from(PLAN.drain_s), &levels);
    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Settling
    );
    assert!(!bench.outputs.switched_on(PumpId::Makeup));

    bench.service.set_supervision(Supervision::Unavailable);
    bench.run(u64::from(PLAN.settle_s + PLAN.refill_s), &levels);

    assert!(
        !bench.outputs.switched_on(PumpId::Makeup),
        "makeup switched on after watchdog supervision was lost"
    );
}
