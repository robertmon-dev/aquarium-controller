use crate::domain::pump::fixtures::*;
use crate::domain::pump::*;

#[test]
fn a_top_off_is_a_single_refilling_phase() {
    let mut operation = Operation::start(OperationPlan::top_off(3), Some(100));

    assert_eq!(operation.phase, Phase::Refilling);
    assert_eq!(operation.remaining_s(), 3);
    assert_eq!(run(&mut operation, 2, HEALTHY), Step::Continue);
    assert_eq!(
        operation.advance(SECOND, SECOND, Some(HEALTHY)),
        Step::Finished(OperationResult::Completed)
    );

    let record = operation.record(OperationResult::Completed);
    assert_eq!(record.kind, OperationKind::TopOff);
    assert_eq!(record.started_at, Some(100));
    assert_eq!(
        (record.drain_ran_s, record.settle_ran_s, record.refill_ran_s),
        (0, 0, 3)
    );
    assert!(!record.resumed);
}

#[test]
fn a_top_off_stops_on_overflow_or_an_empty_reservoir() {
    let mut overflow = Operation::start(OperationPlan::top_off(60), None);
    assert_eq!(
        overflow.advance(
            SECOND,
            SECOND,
            Some(LevelConditions {
                tank_overflow: true,
                ..HEALTHY
            })
        ),
        Step::Finished(OperationResult::Overflow)
    );

    let mut empty = Operation::start(OperationPlan::top_off(60), None);
    assert_eq!(
        empty.advance(
            SECOND,
            SECOND,
            Some(LevelConditions {
                reservoir_empty: true,
                ..HEALTHY
            })
        ),
        Step::Finished(OperationResult::ReservoirEmpty)
    );
}

#[test]
fn a_full_sump_ends_the_refill_but_not_the_drain() {
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };

    let mut top_off = Operation::start(OperationPlan::top_off(60), None);
    assert_eq!(
        top_off.advance(0, 0, Some(full)),
        Step::Finished(OperationResult::SumpFull)
    );

    let mut draining = Operation::start(plan(60, 0, 60), None);
    assert_eq!(draining.advance(SECOND, SECOND, Some(full)), Step::Continue);
}

#[test]
fn a_full_sump_ends_a_water_change_refill_only_after_the_makeup_ran() {
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    let mut operation = Operation::start(plan(1, 0, 60), None);
    run(&mut operation, 1, full);
    run(&mut operation, 1, full);
    assert_eq!(operation.phase, Phase::Refilling);

    for _ in 0..5 {
        assert_eq!(operation.advance(0, 0, Some(full)), Step::Continue);
    }
    assert_eq!(operation.ran_s(Phase::Refilling), 0);

    assert_eq!(
        operation.advance(SECOND, SECOND, Some(HEALTHY)),
        Step::Continue
    );
    assert_eq!(
        operation.advance(0, 0, Some(full)),
        Step::Finished(OperationResult::SumpFull)
    );
}

#[test]
fn the_refill_budget_is_twice_the_refill_or_five_minutes_more() {
    assert_eq!(refill_timeout_s(1), 301);
    assert_eq!(refill_timeout_s(60), 360);
    assert_eq!(refill_timeout_s(300), 600);
    assert_eq!(refill_timeout_s(420), 840);
    assert_eq!(refill_timeout_s(MAX_REFILL_S), 1800);
}

#[test]
fn a_stalled_water_change_refill_times_out_on_wall_time() {
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    let mut operation = Operation::start(plan(10, 30, 60), None);
    run(&mut operation, 10, HEALTHY);
    run(&mut operation, 30, HEALTHY);
    assert_eq!(operation.phase, Phase::Refilling);
    assert_eq!(operation.refill_wall_ms, 0);

    for _ in 0..360 {
        assert_eq!(operation.advance(0, SECOND, Some(full)), Step::Continue);
    }
    assert_eq!(operation.ran_s(Phase::Refilling), 0);
    assert_eq!(operation.refill_wall_ms, 360 * SECOND);

    assert_eq!(
        operation.advance(0, SECOND, Some(full)),
        Step::Finished(OperationResult::RefillTimeout)
    );
    assert_eq!(
        operation.record(OperationResult::RefillTimeout).result,
        OperationResult::RefillTimeout
    );
}

#[test]
fn a_stalled_top_off_times_out_on_wall_time() {
    let mut operation = Operation::start(OperationPlan::top_off(600), None);

    assert_eq!(
        operation.advance(0, 1200 * SECOND, Some(HEALTHY)),
        Step::Continue
    );
    assert_eq!(
        operation.advance(0, 1, Some(HEALTHY)),
        Step::Finished(OperationResult::RefillTimeout)
    );
}

#[test]
fn wall_time_counts_only_while_refilling() {
    let mut operation = Operation::start(plan(2, 2, 60), None);

    assert_eq!(
        operation.advance(0, 300 * SECOND, Some(HEALTHY)),
        Step::Continue
    );
    assert_eq!(operation.refill_wall_ms, 0);
    run(&mut operation, 2, HEALTHY);
    assert_eq!(operation.phase, Phase::Settling);
    assert_eq!(
        operation.advance(2 * SECOND, 2 * SECOND, Some(HEALTHY)),
        Step::PhaseChanged
    );
    assert_eq!(operation.phase, Phase::Refilling);
    assert_eq!(operation.refill_wall_ms, 0);
}

#[test]
fn a_finished_refill_completes_before_the_budget_is_checked() {
    let mut operation = Operation::start(OperationPlan::top_off(60), None);

    assert_eq!(
        operation.advance(60 * SECOND, 400 * SECOND, Some(HEALTHY)),
        Step::Finished(OperationResult::Completed)
    );
}

#[test]
fn resuming_keeps_the_refill_wall_time() {
    let mut operation = Operation::start(OperationPlan::top_off(60), None);
    operation.advance(0, 300 * SECOND, Some(HEALTHY));

    operation.resume();

    assert_eq!(operation.refill_wall_ms, 300 * SECOND);
    assert_eq!(
        operation.advance(0, 61 * SECOND, Some(HEALTHY)),
        Step::Finished(OperationResult::RefillTimeout)
    );
}

#[test]
fn a_checkpoint_without_refill_wall_time_loads_as_zero() {
    let mut operation = Operation::start(OperationPlan::top_off(60), Some(42));
    operation.advance(4200, 4200, Some(HEALTHY));
    let mut json = serde_json::to_value(Checkpoint {
        operation,
        checkpoint_at: Some(50),
    })
    .expect("encode");
    json["operation"]
        .as_object_mut()
        .expect("operation object")
        .remove("refill_wall_ms")
        .expect("refill_wall_ms stored");

    let decoded: Checkpoint = serde_json::from_value(json).expect("decode");

    assert_eq!(decoded.operation.refill_wall_ms, 0);
    assert_eq!(decoded.operation.ran_ms[Phase::Refilling.index()], 4200);
}

#[test]
fn a_drain_that_never_runs_times_out_on_wall_time() {
    let mut operation = Operation::start(plan(60, 2, 60), None);

    assert_eq!(
        operation.advance(0, u64::from(phase_timeout_s(60)) * SECOND, Some(HEALTHY)),
        Step::Continue
    );
    assert_eq!(
        operation.advance(0, 1, Some(HEALTHY)),
        Step::Finished(OperationResult::DrainTimeout)
    );
    assert_eq!(
        operation.record(OperationResult::DrainTimeout).drain_ran_s,
        0
    );
}

#[test]
fn a_running_drain_settles_before_the_wall_time_budget() {
    let mut operation = Operation::start(plan(600, 2, 60), None);

    assert_eq!(
        operation.advance(599 * SECOND, 599 * SECOND, Some(HEALTHY)),
        Step::Continue
    );
    assert_eq!(
        operation.advance(SECOND, 700 * SECOND, Some(HEALTHY)),
        Step::PhaseChanged
    );
    assert_eq!(operation.phase, Phase::Settling);
}

#[test]
fn a_checkpoint_without_drain_wall_time_loads_as_zero() {
    let mut operation = Operation::start(plan(60, 2, 60), Some(42));
    operation.advance(4200, 4200, Some(HEALTHY));
    let mut json = serde_json::to_value(Checkpoint {
        operation,
        checkpoint_at: Some(50),
    })
    .expect("encode");
    json["operation"]
        .as_object_mut()
        .expect("operation object")
        .remove("drain_wall_ms")
        .expect("drain_wall_ms stored");

    let decoded: Checkpoint = serde_json::from_value(json).expect("decode");

    assert_eq!(decoded.operation.drain_wall_ms, 0);
    assert_eq!(decoded.operation.ran_ms[Phase::Draining.index()], 4200);
}

#[test]
fn a_water_change_drains_settles_and_refills() {
    let mut operation = Operation::start(plan(3, 2, 4), Some(1));
    assert_eq!(operation.phase, Phase::Draining);

    assert_eq!(run(&mut operation, 3, HEALTHY), Step::PhaseChanged);
    assert_eq!(operation.phase, Phase::Settling);

    assert_eq!(run(&mut operation, 2, HEALTHY), Step::PhaseChanged);
    assert_eq!(operation.phase, Phase::Refilling);

    assert_eq!(
        run(&mut operation, 4, HEALTHY),
        Step::Finished(OperationResult::Completed)
    );

    let record = operation.record(OperationResult::Completed);
    assert_eq!(
        (record.drain_ran_s, record.settle_ran_s, record.refill_ran_s),
        (3, 2, 4)
    );
}

#[test]
fn a_low_sump_does_not_end_the_drain() {
    let low = LevelConditions {
        sump_low: true,
        ..HEALTHY
    };
    let mut operation = Operation::start(plan(60, 30, 60), None);

    assert_eq!(run(&mut operation, 59, low), Step::Continue);
    assert_eq!(operation.phase, Phase::Draining);
    assert_eq!(
        operation.advance(SECOND, SECOND, Some(low)),
        Step::PhaseChanged
    );
    assert_eq!(operation.phase, Phase::Settling);
    assert_eq!(operation.ran_s(Phase::Draining), 60);
}

#[test]
fn missing_levels_abort_any_phase() {
    let mut operation = Operation::start(plan(60, 30, 60), None);

    assert_eq!(
        operation.advance(SECOND, SECOND, None),
        Step::Finished(OperationResult::Aborted)
    );
}

#[test]
fn remaining_time_rounds_up() {
    let mut operation = Operation::start(plan(10, 0, 10), None);
    operation.advance(1500, 1500, Some(HEALTHY));

    assert_eq!(operation.remaining_s(), 9);
}

#[test]
fn a_high_sump_ends_any_refill_with_an_overflow() {
    let high = LevelConditions {
        sump_high: true,
        ..HEALTHY
    };

    let mut top_off = Operation::start(OperationPlan::top_off(60), None);
    assert_eq!(
        top_off.advance(0, 0, Some(high)),
        Step::Finished(OperationResult::Overflow)
    );

    let mut change = Operation::start(plan(1, 0, 60), None);
    assert_eq!(
        change.advance(SECOND, SECOND, Some(high)),
        Step::PhaseChanged
    );
    assert_eq!(change.advance(0, 0, Some(high)), Step::PhaseChanged);
    assert_eq!(
        change.advance(0, 0, Some(high)),
        Step::Finished(OperationResult::Overflow)
    );

    let mut armed = Operation::start(OperationPlan::top_off(60), None);
    assert_eq!(
        armed.advance(
            0,
            0,
            Some(LevelConditions {
                sump_full: true,
                ..high
            })
        ),
        Step::Finished(OperationResult::SumpFull)
    );
}

#[test]
fn a_high_sump_does_not_end_the_drain() {
    let mut draining = Operation::start(plan(60, 0, 60), None);
    assert_eq!(
        draining.advance(
            SECOND,
            SECOND,
            Some(LevelConditions {
                sump_high: true,
                ..HEALTHY
            })
        ),
        Step::Continue
    );
}
