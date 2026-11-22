use crate::domain::reservoir::fixtures::*;
use crate::domain::reservoir::*;

fn budget_plan() -> FillPlan {
    ReservoirSettings {
        target_tds_ppm: None,
        empty_fill_s: 1000,
        max_duration_s: 43_200,
        ..TARGET
    }
    .fill_plan()
}

#[test]
fn inlet_time_beyond_the_budget_is_an_overtime_fault() {
    let mut fill = Fill::start(FillTrigger::Auto, None, budget_plan());
    step(&mut fill, barrel(false, false), 0, IDLE, None);

    assert_eq!(
        step(&mut fill, barrel(false, false), 1_200_000, RO_ON, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, false), 1, RO_ON, None),
        FillStep::Finished(FillResult::Overtime)
    );
    assert_eq!(FillResult::Overtime.fault(), Some(FillFault::Overtime));
}

#[test]
fn waiting_time_does_not_count_towards_the_overtime_budget() {
    let mut fill = Fill::start(FillTrigger::Auto, None, budget_plan());
    step(&mut fill, barrel(false, false), 0, IDLE, None);

    assert_eq!(
        step(&mut fill, barrel(false, false), 10_000_000, IDLE, None),
        FillStep::Continue
    );
}

#[test]
fn wall_time_beyond_max_duration_times_out() {
    let mut fill = measuring(plan());
    step(&mut fill, barrel(false, false), 0, MIXING, Some(120.0));
    assert_eq!(
        step(&mut fill, barrel(false, false), 25_200_001, IDLE, None),
        FillStep::Finished(FillResult::Timeout)
    );
}

#[test]
fn no_reading_after_mixing_gives_up_after_thirty_seconds() {
    let mut fill = measuring(plan());
    assert_eq!(
        step(&mut fill, barrel(false, false), 29_999, MIXING, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, false), 2, MIXING, None),
        FillStep::Finished(FillResult::TdsUnavailable)
    );
}

#[test]
fn a_leak_or_a_barrel_emptied_again_ends_the_fill() {
    let mut leaking = measuring(plan());
    let wet = ReservoirConditions {
        leak: Some(true),
        ..barrel(false, false)
    };
    assert_eq!(
        step(&mut leaking, wet, 0, IDLE, None),
        FillStep::Finished(FillResult::Leak)
    );

    let mut drained = measuring(plan());
    assert_eq!(
        step(&mut drained, barrel(true, false), 10_000, IDLE, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut drained, barrel(true, false), 1, IDLE, None),
        FillStep::Finished(FillResult::Blocked)
    );
}

#[test]
fn overtime_is_not_raised_while_the_barrel_reads_full() {
    let mut fill = Fill::start(FillTrigger::Auto, None, budget_plan());
    step(&mut fill, barrel(false, false), 0, IDLE, None);

    assert_eq!(
        step(&mut fill, barrel(false, true), 1_300_000, RO_ON, None),
        FillStep::Finished(FillResult::Completed)
    );
}
