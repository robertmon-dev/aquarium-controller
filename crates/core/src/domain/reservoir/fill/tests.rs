use crate::domain::reservoir::fixtures::*;
use crate::domain::reservoir::*;

fn ro_only() -> FillPlan {
    ReservoirSettings {
        target_tds_ppm: None,
        ..TARGET
    }
    .fill_plan()
}

#[test]
fn an_empty_barrel_primes_with_ro_then_mixes_and_measures() {
    let mut fill = Fill::start(FillTrigger::Manual, Some(1), plan());
    assert_eq!(
        fill.demand(),
        Demand {
            inlet: Some(Inlet::Ro),
            mixer: false
        }
    );

    assert_eq!(
        step(&mut fill, barrel(true, false), 1000, RO_ON, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, false), 1000, RO_ON, None),
        FillStep::PhaseChanged
    );
    assert_eq!(fill.phase(), FillPhase::Mixing);
    assert_eq!(fill.pulses(), [0, 0]);
    assert_eq!(
        fill.demand(),
        Demand {
            inlet: None,
            mixer: true
        }
    );

    assert_eq!(
        step(&mut fill, barrel(false, false), 59_000, MIXING, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, false), 1000, MIXING, None),
        FillStep::PhaseChanged
    );
    assert_eq!(fill.phase(), FillPhase::Measuring);
}

#[test]
fn priming_an_empty_barrel_is_not_cut_short_by_the_empty_again_guard() {
    let mut fill = Fill::start(FillTrigger::Manual, None, plan());
    assert_eq!(
        step(&mut fill, barrel(true, false), 11_000, RO_ON, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(true, false), 600_000, RO_ON, None),
        FillStep::Continue
    );
    assert_eq!(fill.phase(), FillPhase::Priming);
}

#[test]
fn mixing_does_not_progress_while_the_mixer_is_off() {
    let mut fill = Fill::start(FillTrigger::Manual, None, plan());
    step(&mut fill, barrel(false, false), 0, IDLE, None);
    assert_eq!(fill.phase(), FillPhase::Mixing);

    assert_eq!(
        step(&mut fill, barrel(false, false), 120_000, IDLE, None),
        FillStep::Continue
    );
    assert_eq!(fill.phase(), FillPhase::Mixing);
}

#[test]
fn a_soft_reading_doses_tap_water_and_a_hard_one_doses_ro() {
    let mut soft = measuring(plan());
    assert_eq!(
        step(&mut soft, barrel(false, false), 2000, MIXING, Some(120.0)),
        FillStep::PhaseChanged
    );
    assert_eq!(soft.dosing(), Some(Inlet::Tap));
    assert_eq!(soft.pulses(), [0, 1]);

    let mut centred = measuring(plan());
    step(
        &mut centred,
        barrel(false, false),
        2000,
        MIXING,
        Some(145.0),
    );
    assert_eq!(
        centred.dosing(),
        Some(Inlet::Tap),
        "dosing tracks the target, not the tolerance edge"
    );

    let mut hard = measuring(plan());
    step(&mut hard, barrel(false, false), 2000, MIXING, Some(150.0));
    assert_eq!(hard.dosing(), Some(Inlet::Ro));
    assert_eq!(hard.pulses(), [1, 0]);
}

#[test]
fn a_pulse_counts_only_while_its_valve_runs() {
    let mut fill = measuring(plan());
    step(&mut fill, barrel(false, false), 0, MIXING, Some(120.0));

    assert_eq!(
        step(&mut fill, barrel(false, false), 60_000, IDLE, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, false), 4000, TAP_MIXING, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, false), 1000, TAP_MIXING, None),
        FillStep::PhaseChanged
    );
    assert_eq!(fill.phase(), FillPhase::Mixing);
}

#[test]
fn reaching_full_mixes_once_more_and_grades_the_last_reading() {
    for (tds, result) in [
        (155.0, FillResult::Completed),
        (175.0, FillResult::OffTarget),
    ] {
        let mut fill = measuring(plan());
        step(&mut fill, barrel(false, false), 0, MIXING, Some(160.0));
        assert_eq!(fill.dosing(), Some(Inlet::Ro));
        assert_eq!(
            step(&mut fill, barrel(false, true), 1000, RO_ON, None),
            FillStep::PhaseChanged
        );
        assert!(fill.finishing());
        step(&mut fill, barrel(false, true), 60_000, MIXING, None);
        assert_eq!(
            step(&mut fill, barrel(false, true), 2000, MIXING, Some(tds)),
            FillStep::Finished(result)
        );
    }
}

#[test]
fn a_reading_taken_in_a_full_barrel_is_graded_without_another_pulse() {
    for (tds, result) in [
        (155.0, FillResult::Completed),
        (175.0, FillResult::OffTarget),
    ] {
        let mut fill = measuring(plan());
        assert_eq!(
            step(&mut fill, barrel(false, true), 2000, MIXING, Some(tds)),
            FillStep::Finished(result)
        );
        assert_eq!(fill.pulses(), [0, 0]);
    }
}

#[test]
fn ro_only_fills_continuously_and_completes_at_full() {
    let mut fill = Fill::start(FillTrigger::Auto, None, ro_only());
    step(&mut fill, barrel(false, false), 0, IDLE, None);
    assert_eq!(fill.dosing(), Some(Inlet::Ro));
    assert_eq!(fill.pulses(), [0, 0]);

    assert_eq!(
        step(&mut fill, barrel(false, false), 3_600_000, RO_ON, None),
        FillStep::Continue
    );
    assert_eq!(
        step(&mut fill, barrel(false, true), 1000, RO_ON, None),
        FillStep::Finished(FillResult::Completed)
    );
    assert_eq!(fill.record(FillResult::Completed, None, None).ro_pulses, 0);
}

#[test]
fn a_running_fill_keeps_the_plan_it_started_with() {
    let mut pulsing = measuring(plan());
    step(&mut pulsing, barrel(false, false), 0, MIXING, Some(120.0));
    assert_eq!(
        step(&mut pulsing, barrel(false, false), 5000, TAP_MIXING, None),
        FillStep::PhaseChanged
    );

    let mut continuous = Fill::start(FillTrigger::Auto, None, ro_only());
    step(&mut continuous, barrel(false, false), 0, IDLE, None);
    assert_eq!(
        step(&mut continuous, barrel(false, false), 130_000, RO_ON, None),
        FillStep::Continue
    );
    assert_eq!(continuous.phase(), FillPhase::Dosing);
}

#[test]
fn a_full_barrel_at_start_only_mixes_and_measures() {
    let mut fill = Fill::start(FillTrigger::Manual, None, plan());
    assert_eq!(
        step(&mut fill, barrel(false, true), 0, IDLE, None),
        FillStep::PhaseChanged
    );
    assert_eq!(fill.phase(), FillPhase::Mixing);
    assert!(fill.finishing());
}

#[test]
fn the_record_sums_the_inlet_time_and_keeps_the_last_reading() {
    let mut fill = measuring(plan());
    step(&mut fill, barrel(false, false), 0, MIXING, Some(120.0));
    step(&mut fill, barrel(false, false), 5000, TAP_MIXING, None);

    let record = fill.record(FillResult::Aborted, Some(99), None);
    assert_eq!(record.tap_s, 5);
    assert_eq!(record.tap_pulses, 1);
    assert_eq!(record.final_tds_ppm, Some(120.0));
    assert_eq!(record.finished_at, Some(99));
    assert_eq!(record.result, FillResult::Aborted);
}

#[test]
fn the_record_prefers_the_fills_own_reading_and_falls_back_to_the_latest() {
    let mut fill = measuring(plan());
    step(&mut fill, barrel(false, false), 0, MIXING, Some(120.0));
    assert_eq!(
        fill.record(FillResult::Aborted, None, Some(88.0))
            .final_tds_ppm,
        Some(120.0)
    );

    let unread = Fill::start(FillTrigger::Manual, None, plan());
    assert_eq!(
        unread
            .record(FillResult::Aborted, None, Some(88.0))
            .final_tds_ppm,
        Some(88.0)
    );
    assert_eq!(
        unread.record(FillResult::Aborted, None, None).final_tds_ppm,
        None
    );
}

#[test]
fn phase_codes_run_from_idle_to_dosing() {
    let phases = [
        FillPhase::Idle,
        FillPhase::Priming,
        FillPhase::Mixing,
        FillPhase::Measuring,
        FillPhase::Dosing,
    ];
    for (code, phase) in phases.into_iter().enumerate() {
        assert_eq!(usize::from(phase.code()), code);
    }
}
