use crate::domain::pump::fixtures::*;
use crate::domain::pump::*;

#[test]
fn resuming_restarts_the_settling_pause_only() {
    let mut operation = Operation::start(plan(2, 30, 60), None);
    run(&mut operation, 2, HEALTHY);
    run(&mut operation, 10, HEALTHY);
    assert_eq!(operation.phase, Phase::Settling);

    operation.resume();

    assert!(operation.resumed);
    assert_eq!(operation.ran_s(Phase::Settling), 0);
    assert_eq!(operation.ran_s(Phase::Draining), 2);
}

#[test]
fn resume_readiness_depends_on_the_phase() {
    let draining = Operation::start(plan(10, 0, 10), None);
    assert_eq!(
        draining.ready_to_resume(None),
        Err(Blocker::LevelsUnavailable)
    );
    assert_eq!(
        draining.ready_to_resume(Some(LevelConditions {
            reservoir_empty: true,
            ..HEALTHY
        })),
        Ok(())
    );

    let refilling = Operation::start(OperationPlan::top_off(10), None);
    assert_eq!(
        refilling.ready_to_resume(Some(LevelConditions {
            reservoir_empty: true,
            ..HEALTHY
        })),
        Err(Blocker::ReservoirEmpty)
    );
    assert_eq!(
        refilling.ready_to_resume(Some(LevelConditions {
            tank_overflow: true,
            ..HEALTHY
        })),
        Err(Blocker::TankOverflow)
    );
}

#[test]
fn resume_verdicts() {
    use ResumeVerdict::{Resume, Suspend, Wait};

    assert_eq!(
        resume_verdict(Some(1000), Some(1100), 20, 600, true),
        Resume
    );
    assert_eq!(
        resume_verdict(Some(1000), Some(1600), 20, 600, true),
        Suspend
    );
    assert_eq!(
        resume_verdict(Some(2000), Some(1100), 20, 600, true),
        Suspend
    );
    assert_eq!(resume_verdict(None, Some(1100), 20, 600, true), Suspend);
    assert_eq!(resume_verdict(Some(1000), Some(1100), 20, 0, true), Suspend);
    assert_eq!(resume_verdict(Some(1000), None, 20, 600, true), Wait);
    assert_eq!(resume_verdict(Some(1000), None, 600, 600, true), Suspend);
    assert_eq!(resume_verdict(Some(1000), Some(1100), 20, 600, false), Wait);
    assert_eq!(
        resume_verdict(Some(1000), Some(1100), 600, 600, false),
        Suspend
    );
}

#[test]
fn litres_need_a_flow_rate() {
    assert_eq!(litres(360, None), None);
    assert_eq!(litres(360, Some(600)), Some(60.0));
}

#[test]
fn checkpoints_round_trip_through_json() {
    let mut operation = Operation::start(plan(10, 5, 10), Some(42));
    operation.advance(4200, 4200, Some(HEALTHY));
    let checkpoint = Checkpoint {
        operation,
        checkpoint_at: Some(50),
    };

    let bytes = serde_json::to_vec(&checkpoint).expect("encode");
    let decoded: Checkpoint = serde_json::from_slice(&bytes).expect("decode");

    assert_eq!(decoded, checkpoint);
}

#[test]
fn a_resumed_refill_waits_for_the_makeup_pump_before_a_full_sump_ends_it() {
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    let mut operation = Operation::start(plan(1, 0, 60), None);
    operation.advance(SECOND, SECOND, Some(HEALTHY));
    operation.advance(0, 0, Some(HEALTHY));
    assert_eq!(operation.phase, Phase::Refilling);
    operation.advance(5 * SECOND, 5 * SECOND, Some(HEALTHY));

    let mut resumed: Operation =
        serde_json::from_slice(&serde_json::to_vec(&operation).expect("encode")).expect("decode");
    resumed.resume();

    assert_eq!(resumed.advance(0, SECOND, Some(full)), Step::Continue);
    assert_eq!(
        resumed.advance(SECOND, SECOND, Some(HEALTHY)),
        Step::Continue
    );
    assert_eq!(
        resumed.advance(0, SECOND, Some(full)),
        Step::Finished(OperationResult::SumpFull)
    );
}

const FLOWS: FlowRates = FlowRates {
    makeup: Some(600),
    drain: Some(1500),
};

fn change_record(drain_ran_s: u32, refill_ran_s: u32, result: OperationResult) -> OperationRecord {
    OperationRecord {
        kind: OperationKind::WaterChange,
        started_at: Some(100),
        drain_ran_s,
        settle_ran_s: 30,
        refill_ran_s,
        result,
        resumed: false,
        finished_at: None,
    }
}

#[test]
fn a_refill_that_added_less_than_seventy_percent_of_the_drain_is_incomplete() {
    let short = change_record(100, 174, OperationResult::Completed).graded(FLOWS, false);
    assert_eq!(short.result, OperationResult::Incomplete);

    let enough = change_record(100, 175, OperationResult::Completed).graded(FLOWS, false);
    assert_eq!(enough.result, OperationResult::Completed);
}

#[test]
fn a_refill_that_ended_on_the_stop_sensor_is_never_incomplete() {
    for sump_low in [false, true] {
        assert_eq!(
            change_record(100, 0, OperationResult::SumpFull)
                .graded(FLOWS, sump_low)
                .result,
            OperationResult::SumpFull
        );
    }
}

#[test]
fn only_refills_that_ran_out_their_time_are_graded() {
    for result in [
        OperationResult::SumpFull,
        OperationResult::Overflow,
        OperationResult::ReservoirEmpty,
        OperationResult::Aborted,
        OperationResult::InterruptedByReboot,
        OperationResult::RefillTimeout,
    ] {
        assert_eq!(
            change_record(100, 0, result).graded(FLOWS, true).result,
            result
        );
    }
}

#[test]
fn the_volume_rule_needs_both_flow_rates() {
    for flows in [
        FlowRates {
            makeup: None,
            drain: Some(1500),
        },
        FlowRates {
            makeup: Some(600),
            drain: None,
        },
        FlowRates::default(),
    ] {
        assert_eq!(
            change_record(100, 0, OperationResult::SumpFull)
                .graded(flows, false)
                .result,
            OperationResult::SumpFull
        );
    }
}

#[test]
fn a_refill_that_ends_with_the_sump_still_low_is_incomplete() {
    assert_eq!(
        change_record(100, 600, OperationResult::Completed)
            .graded(FlowRates::default(), true)
            .result,
        OperationResult::Incomplete
    );
    assert_eq!(
        change_record(100, 600, OperationResult::Completed)
            .graded(FLOWS, false)
            .result,
        OperationResult::Completed
    );
}

#[test]
fn top_offs_are_never_graded() {
    let record = OperationRecord {
        kind: OperationKind::TopOff,
        ..change_record(0, 10, OperationResult::Completed)
    };

    assert_eq!(
        record.graded(FLOWS, true).result,
        OperationResult::Completed
    );
}

#[test]
fn an_incomplete_result_is_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_string(&OperationResult::Incomplete).expect("encode"),
        r#""incomplete""#
    );
}

fn finished_change(started_at: Option<u64>, drain_ms: u64) -> (Operation, OperationRecord) {
    let mut operation = Operation::start(plan(10, 0, 10), started_at);
    operation.ran_ms[Phase::Draining.index()] = drain_ms;
    let record = operation.record(OperationResult::Aborted);
    (operation, record)
}

#[test]
fn only_a_water_change_that_drained_sets_the_gap() {
    let mut last = LastOperations::default();

    let (operation, record) = finished_change(Some(100), 0);
    last.remember(&operation, record);
    assert_eq!(last.water_change, Some(record));
    assert_eq!(last.last_drain_started_at(), None);

    let (operation, record) = finished_change(Some(200), 400);
    last.remember(&operation, record);
    assert_eq!(last.last_drain_started_at(), Some(200));

    let (operation, record) = finished_change(Some(300), 0);
    last.remember(&operation, record);
    assert_eq!(last.water_change, Some(record));
    assert_eq!(last.last_drain_started_at(), Some(200));
}

#[test]
fn a_drained_water_change_without_a_start_time_clears_the_gap() {
    let mut last = LastOperations::default();
    let (operation, record) = finished_change(Some(200), 4000);
    last.remember(&operation, record);

    let (operation, record) = finished_change(None, 4000);
    last.remember(&operation, record);

    assert_eq!(last.last_drain_started_at(), None);
}

#[test]
fn a_top_off_leaves_the_gap_alone() {
    let mut last = LastOperations::default();
    let (operation, record) = finished_change(Some(200), 4000);
    last.remember(&operation, record);

    let mut top_off = Operation::start(OperationPlan::top_off(60), Some(300));
    top_off.ran_ms[Phase::Refilling.index()] = 60_000;
    last.remember(&top_off, top_off.record(OperationResult::Completed));

    assert_eq!(last.last_drain_started_at(), Some(200));
    assert!(last.top_off.is_some());
}

#[test]
fn history_stored_before_the_gap_mark_falls_back_to_the_last_water_change() {
    let drained: LastOperations = serde_json::from_str(
        r#"{"top_off":null,"water_change":{"kind":"water_change","started_at":500,
            "drain_ran_s":3,"settle_ran_s":0,"refill_ran_s":0,"result":"aborted","resumed":false}}"#,
    )
    .expect("old history");
    assert_eq!(drained.last_drain_started_at(), Some(500));

    let undrained: LastOperations = serde_json::from_str(
        r#"{"top_off":null,"water_change":{"kind":"water_change","started_at":500,
            "drain_ran_s":0,"settle_ran_s":0,"refill_ran_s":0,"result":"aborted","resumed":false}}"#,
    )
    .expect("old history");
    assert_eq!(undrained.last_drain_started_at(), None);
}

#[test]
fn a_water_mover_does_not_resume_into_a_high_sump() {
    let high = Some(LevelConditions {
        sump_high: true,
        ..HEALTHY
    });

    assert_eq!(
        Operation::start(OperationPlan::top_off(10), None).ready_to_resume(high),
        Err(Blocker::SumpHigh)
    );
    assert_eq!(
        Operation::start(plan(10, 0, 10), None).ready_to_resume(high),
        Err(Blocker::SumpHigh)
    );
}

#[test]
fn a_stored_override_needs_an_explicit_expiry() {
    assert!(serde_json::from_str::<StoredOverride>("{}").is_err());
    assert_eq!(
        serde_json::from_str::<StoredOverride>(r#"{"expires_at":null}"#).expect("indefinite"),
        StoredOverride { expires_at: None }
    );
    assert_eq!(
        serde_json::from_str::<StoredOverride>(r#"{"expires_at":42}"#).expect("timed"),
        StoredOverride {
            expires_at: Some(42)
        }
    );
    assert_eq!(
        serde_json::to_string(&StoredOverride { expires_at: None }).expect("encode"),
        r#"{"expires_at":null}"#
    );
}
