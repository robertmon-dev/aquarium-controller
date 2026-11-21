use super::*;
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    reservoir_ready: true,
    tank_overflow: false,
    sump_full: false,
};

fn snapshot(wet: &[WaterLevelRole], faulty: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = WaterLevelSnapshot::dry();

    for reading in &mut snapshot.sensors {
        if wet.contains(&reading.role) {
            reading.state = WaterLevelState::Wet;
        }
        reading.faulty = faulty.contains(&reading.role);
    }

    snapshot
}

fn input(levels: Option<LevelConditions>) -> DecisionInput {
    DecisionInput {
        levels,
        ..DecisionInput::default()
    }
}

fn state(decisions: [ChannelDecision; 4]) -> [bool; 4] {
    decisions.map(|decision| decision.on)
}

#[test]
fn pump_names_round_trip() {
    for id in PumpId::ALL {
        assert_eq!(id.name().parse::<PumpId>(), Ok(id));
    }

    assert!("filter".parse::<PumpId>().is_err());
    assert_eq!(PumpId::Return.override_slot(), Some(0));
    assert_eq!(PumpId::Uv.override_slot(), Some(1));
    assert_eq!(PumpId::Makeup.override_slot(), None);
    assert_eq!(PumpId::Drain.override_slot(), None);
}

#[test]
fn level_conditions_treat_faulty_sensors_as_dangerous() {
    let wet_sump_and_reserve = snapshot(
        &[
            WaterLevelRole::SumpPump,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::ReservoirReady,
        ],
        &[],
    );
    assert_eq!(
        LevelConditions::from_snapshot(&wet_sump_and_reserve, None),
        HEALTHY
    );

    let all_dry = LevelConditions::from_snapshot(&WaterLevelSnapshot::dry(), None);
    assert!(all_dry.sump_low && all_dry.reservoir_empty && !all_dry.tank_overflow);
    assert!(!all_dry.reservoir_ready);

    let faulty = snapshot(
        &[
            WaterLevelRole::SumpPump,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::ReservoirReady,
        ],
        &[
            WaterLevelRole::SumpPump,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::MainTank,
            WaterLevelRole::ReservoirReady,
        ],
    );
    assert_eq!(
        LevelConditions::from_snapshot(&faulty, None),
        LevelConditions {
            sump_low: true,
            reservoir_empty: true,
            reservoir_ready: false,
            tank_overflow: true,
            sump_full: false,
        }
    );
}

#[test]
fn only_the_ready_sensor_says_the_reservoir_holds_a_water_change() {
    let ready = snapshot(&[WaterLevelRole::ReservoirReady], &[]);
    assert!(LevelConditions::from_snapshot(&ready, None).reservoir_ready);

    let only_full = snapshot(
        &[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirFull],
        &[],
    );
    assert!(!LevelConditions::from_snapshot(&only_full, None).reservoir_ready);

    let faulty = snapshot(
        &[WaterLevelRole::ReservoirReady],
        &[WaterLevelRole::ReservoirReady],
    );
    assert!(!LevelConditions::from_snapshot(&faulty, None).reservoir_ready);
}

#[test]
fn the_refill_stop_sensor_reports_a_full_sump() {
    let wet_biological = snapshot(
        &[
            WaterLevelRole::SumpPump,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::SumpBiological,
        ],
        &[],
    );

    assert!(
        LevelConditions::from_snapshot(&wet_biological, Some(WaterLevelRole::SumpBiological))
            .sump_full
    );
    assert!(
        !LevelConditions::from_snapshot(&wet_biological, Some(WaterLevelRole::SumpMechanical))
            .sump_full
    );
    assert!(!LevelConditions::from_snapshot(&wet_biological, None).sump_full);

    let faulty_mechanical = snapshot(&[], &[WaterLevelRole::SumpMechanical]);
    assert!(
        LevelConditions::from_snapshot(&faulty_mechanical, Some(WaterLevelRole::SumpMechanical))
            .sump_full
    );
}

#[test]
fn makeup_stops_on_a_full_sump_or_a_return_override() {
    let refilling = DecisionInput {
        phase: Some(Phase::Refilling),
        ..input(Some(HEALTHY))
    };

    let full = decide(DecisionInput {
        levels: Some(LevelConditions {
            sump_full: true,
            ..HEALTHY
        }),
        ..refilling
    });
    assert!(!full[2].on);
    assert_eq!(full[2].blocked_by, Some(Blocker::SumpFull));

    let return_off = decide(DecisionInput {
        return_forced_off: true,
        ..refilling
    });
    assert!(!return_off[2].on);
    assert_eq!(return_off[2].blocked_by, Some(Blocker::ReturnOff));

    let locked_out = decide(DecisionInput {
        return_locked_out: true,
        ..refilling
    });
    assert!(locked_out[2].on);
}

#[test]
fn healthy_levels_run_return_and_uv_only() {
    assert_eq!(
        state(decide(input(Some(HEALTHY)))),
        [true, true, false, false]
    );
}

#[test]
fn a_low_sump_stops_return_and_uv() {
    let decisions = decide(input(Some(LevelConditions {
        sump_low: true,
        ..HEALTHY
    })));

    assert_eq!(state(decisions), [false, false, false, false]);
    assert_eq!(decisions[0].blocked_by, Some(Blocker::SumpLow));
    assert_eq!(decisions[1].blocked_by, Some(Blocker::ReturnOff));
}

#[test]
fn the_restart_lockout_keeps_return_off() {
    let decisions = decide(DecisionInput {
        return_locked_out: true,
        ..input(Some(HEALTHY))
    });

    assert!(!decisions[0].on);
    assert_eq!(decisions[0].blocked_by, Some(Blocker::SumpLow));
}

#[test]
fn overrides_switch_return_and_uv_off_without_a_blocker() {
    let return_off = decide(DecisionInput {
        return_forced_off: true,
        ..input(Some(HEALTHY))
    });
    assert_eq!(
        return_off[0],
        ChannelDecision {
            on: false,
            blocked_by: None
        }
    );
    assert_eq!(return_off[1].blocked_by, Some(Blocker::ReturnOff));

    let uv_off = decide(DecisionInput {
        uv_forced_off: true,
        ..input(Some(HEALTHY))
    });
    assert!(uv_off[0].on);
    assert_eq!(
        uv_off[1],
        ChannelDecision {
            on: false,
            blocked_by: None
        }
    );
}

#[test]
fn missing_levels_keep_circulation_and_stop_water_movers() {
    let decisions = decide(DecisionInput {
        phase: Some(Phase::Refilling),
        ..input(None)
    });

    assert_eq!(state(decisions), [true, true, false, false]);
    assert_eq!(decisions[2].blocked_by, Some(Blocker::LevelsUnavailable));
    assert_eq!(decisions[3].blocked_by, Some(Blocker::LevelsUnavailable));
}

#[test]
fn makeup_runs_while_refilling_unless_blocked() {
    let refilling = DecisionInput {
        phase: Some(Phase::Refilling),
        ..input(Some(HEALTHY))
    };
    assert!(decide(refilling)[2].on);

    let empty = decide(DecisionInput {
        levels: Some(LevelConditions {
            reservoir_empty: true,
            ..HEALTHY
        }),
        ..refilling
    });
    assert!(!empty[2].on);
    assert_eq!(empty[2].blocked_by, Some(Blocker::ReservoirEmpty));

    let overflow = decide(DecisionInput {
        levels: Some(LevelConditions {
            tank_overflow: true,
            ..HEALTHY
        }),
        ..refilling
    });
    assert!(!overflow[2].on);
    assert_eq!(overflow[2].blocked_by, Some(Blocker::TankOverflow));
}

#[test]
fn drain_runs_only_while_draining() {
    let draining = decide(DecisionInput {
        phase: Some(Phase::Draining),
        ..input(Some(HEALTHY))
    });
    assert_eq!(state(draining), [false, false, false, true]);

    let settling = decide(DecisionInput {
        phase: Some(Phase::Settling),
        ..input(Some(HEALTHY))
    });
    assert_eq!(state(settling), [false, false, false, false]);
}

#[test]
fn a_water_change_keeps_the_return_pump_off_until_refilling() {
    for phase in [Phase::Draining, Phase::Settling] {
        let decisions = decide(DecisionInput {
            phase: Some(phase),
            ..input(Some(HEALTHY))
        });
        assert_eq!(decisions[0].blocked_by, Some(Blocker::WaterChange));
        assert!(!decisions[0].on);
        assert_eq!(decisions[1].blocked_by, Some(Blocker::ReturnOff));
        assert!(!decisions[1].on);
    }

    let refilling = DecisionInput {
        phase: Some(Phase::Refilling),
        ..input(Some(HEALTHY))
    };
    assert_eq!(state(decide(refilling)), [true, true, true, false]);

    let low = decide(DecisionInput {
        levels: Some(LevelConditions {
            sump_low: true,
            ..HEALTHY
        }),
        ..refilling
    });
    assert_eq!(low[0].blocked_by, Some(Blocker::SumpLow));

    let locked_out = decide(DecisionInput {
        return_locked_out: true,
        ..refilling
    });
    assert_eq!(locked_out[0].blocked_by, Some(Blocker::SumpLow));
    assert!(locked_out[2].on);
}

#[test]
fn missing_levels_keep_circulation_even_while_draining() {
    let decisions = decide(DecisionInput {
        phase: Some(Phase::Draining),
        ..input(None)
    });

    assert_eq!(state(decisions), [true, true, false, false]);
    assert_eq!(decisions[0].blocked_by, None);
}

fn defaults() -> PumpSettings {
    PumpSettings::DEFAULT
}

fn top_off(cron: &str, duration_s: u32) -> TopOffWindow {
    TopOffWindow {
        cron: cron.into(),
        duration_s,
    }
}

fn water_change(cron: &str) -> WaterChangeWindow {
    WaterChangeWindow {
        cron: cron.into(),
        drain_s: 300,
        settle_s: 30,
        refill_s: 420,
    }
}

#[test]
fn default_settings_are_valid() {
    assert_eq!(defaults().validate(), Ok(()));
    assert_eq!(defaults().resume_window_s, DEFAULT_RESUME_WINDOW_S);
}

#[test]
fn settings_validation_covers_every_field() {
    let mut too_many = defaults();
    too_many.top_off = (0..5).map(|_| top_off("0 8 * * *", 60)).collect();
    assert!(too_many.validate().is_err());

    let mut bad_cron = defaults();
    bad_cron.water_change = vec![water_change("0 25 * * *")];
    assert_eq!(bad_cron.validate(), Err("cron value out of range"));

    let mut long_top_off = defaults();
    long_top_off.top_off = vec![top_off("0 8 * * *", MAX_TOP_OFF_S + 1)];
    assert!(long_top_off.validate().is_err());

    let mut long_drain = defaults();
    long_drain.water_change = vec![WaterChangeWindow {
        drain_s: MAX_DRAIN_S + 1,
        ..water_change("0 10 * * sun")
    }];
    assert!(long_drain.validate().is_err());

    let mut long_cron = defaults();
    long_cron.top_off = vec![top_off(&"0 ".repeat(33), 60)];
    assert_eq!(
        long_cron.validate(),
        Err("cron must be at most 64 characters")
    );

    let mut zero_flow = defaults();
    zero_flow.flow_lph.makeup = Some(0);
    assert!(zero_flow.validate().is_err());

    let mut long_window = defaults();
    long_window.resume_window_s = MAX_RESUME_WINDOW_S + 1;
    assert!(long_window.validate().is_err());

    let mut fine = defaults();
    fine.top_off = vec![top_off("0 8,20 * * *", 120)];
    fine.water_change = vec![water_change("0 10 * * sun")];
    fine.flow_lph = FlowRates {
        makeup: Some(300),
        drain: Some(600),
    };
    assert_eq!(fine.validate(), Ok(()));
}

#[test]
fn the_refill_stop_sensor_must_sit_in_the_sump() {
    assert_eq!(
        defaults().refill_stop_sensor,
        Some(WaterLevelRole::SumpReturnLevel)
    );

    for role in [
        WaterLevelRole::SumpReturnLevel,
        WaterLevelRole::SumpMechanical,
        WaterLevelRole::SumpBiological,
    ] {
        let mut settings = defaults();
        settings.refill_stop_sensor = Some(role);
        assert_eq!(settings.validate(), Ok(()), "{role:?}");
    }

    for role in [
        WaterLevelRole::SumpPump,
        WaterLevelRole::MainTank,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::ReservoirFull,
    ] {
        let mut settings = defaults();
        settings.refill_stop_sensor = Some(role);
        assert_eq!(
            settings.validate(),
            Err("refill_stop_sensor must be sump_return_level, sump_mechanical or sump_biological"),
            "{role:?}"
        );
    }
}

#[test]
fn the_water_change_gap_is_bounded() {
    assert_eq!(
        defaults().min_water_change_gap_h,
        DEFAULT_WATER_CHANGE_GAP_H
    );

    for hours in [0, MAX_WATER_CHANGE_GAP_H] {
        let mut settings = defaults();
        settings.min_water_change_gap_h = hours;
        assert_eq!(settings.validate(), Ok(()));
    }

    let mut too_long = defaults();
    too_long.min_water_change_gap_h = MAX_WATER_CHANGE_GAP_H + 1;
    assert_eq!(
        too_long.validate(),
        Err("min_water_change_gap_h must not exceed 168")
    );
}

#[test]
fn a_patch_sets_keeps_or_clears_the_refill_stop_sensor() {
    let mut base = defaults();
    base.refill_stop_sensor = Some(WaterLevelRole::SumpMechanical);

    let set: PumpSettingsPatch =
        serde_json::from_str(r#"{"refill_stop_sensor":"sump_biological"}"#).expect("patch");
    assert_eq!(
        set.merge_onto(&base).refill_stop_sensor,
        Some(WaterLevelRole::SumpBiological)
    );

    let kept: PumpSettingsPatch = serde_json::from_str("{}").expect("patch");
    assert_eq!(
        kept.merge_onto(&base).refill_stop_sensor,
        Some(WaterLevelRole::SumpMechanical)
    );

    let cleared: PumpSettingsPatch =
        serde_json::from_str(r#"{"refill_stop_sensor":null}"#).expect("patch");
    assert_eq!(cleared.merge_onto(&base).refill_stop_sensor, None);

    let gap: PumpSettingsPatch =
        serde_json::from_str(r#"{"min_water_change_gap_h":24}"#).expect("patch");
    assert_eq!(gap.merge_onto(&base).min_water_change_gap_h, 24);
}

#[test]
fn settings_stored_before_the_refill_safeguards_still_load() {
    let stored: PumpSettings = serde_json::from_str(
        r#"{"top_off":[],"water_change":[],"flow_lph":{"makeup":null,"drain":null},"resume_window_s":120}"#,
    )
    .expect("old settings");

    assert_eq!(stored.resume_window_s, 120);
    assert_eq!(
        stored.refill_stop_sensor,
        Some(WaterLevelRole::SumpReturnLevel)
    );
    assert_eq!(stored.min_water_change_gap_h, DEFAULT_WATER_CHANGE_GAP_H);
}

#[test]
fn a_stored_null_refill_stop_sensor_stays_cleared() {
    let stored: PumpSettings = serde_json::from_str(
        r#"{"top_off":[],"water_change":[],"flow_lph":{"makeup":null,"drain":null},"resume_window_s":120,"refill_stop_sensor":null}"#,
    )
    .expect("settings");

    assert_eq!(stored.refill_stop_sensor, None);

    let round_trip: PumpSettings =
        serde_json::from_str(&serde_json::to_string(&stored).expect("encodes")).expect("decodes");
    assert_eq!(round_trip.refill_stop_sensor, None);
}

#[test]
fn a_patch_replaces_only_the_fields_it_carries() {
    let base = defaults();
    let patch = PumpSettingsPatch {
        top_off: Some(vec![top_off("0 8 * * *", 60)]),
        resume_window_s: Some(120),
        ..PumpSettingsPatch::default()
    };

    let merged = patch.merge_onto(&base);

    assert_eq!(merged.top_off, vec![top_off("0 8 * * *", 60)]);
    assert_eq!(merged.water_change, base.water_change);
    assert_eq!(merged.flow_lph, base.flow_lph);
    assert_eq!(merged.resume_window_s, 120);
}

#[test]
fn a_water_change_plan_defaults_its_settling_pause() {
    let plan: WaterChangePlan =
        serde_json::from_str(r#"{"drain_s":10,"refill_s":20}"#).expect("plan");

    assert_eq!(plan.settle_s, DEFAULT_SETTLE_S);
    assert_eq!(plan.validate(), Ok(()));

    for invalid in [
        WaterChangePlan { drain_s: 0, ..plan },
        WaterChangePlan {
            refill_s: 0,
            ..plan
        },
        WaterChangePlan {
            settle_s: MAX_SETTLE_S + 1,
            ..plan
        },
        WaterChangePlan {
            refill_s: MAX_REFILL_S + 1,
            ..plan
        },
    ] {
        assert!(invalid.validate().is_err(), "{invalid:?}");
    }
}

#[test]
fn override_rules_differ_per_channel() {
    assert_eq!(
        validate_override(PumpId::Return, None),
        Err(OverrideError::Invalid(
            "a return pump override needs duration_s"
        ))
    );
    assert!(validate_override(PumpId::Return, Some(59)).is_err());
    assert!(validate_override(PumpId::Return, Some(MAX_RETURN_OVERRIDE_S + 1)).is_err());
    assert_eq!(validate_override(PumpId::Return, Some(600)), Ok(()));

    assert_eq!(validate_override(PumpId::Uv, None), Ok(()));
    assert!(validate_override(PumpId::Uv, Some(MAX_UV_OVERRIDE_S + 1)).is_err());
    assert_eq!(validate_override(PumpId::Uv, Some(86_400)), Ok(()));

    assert_eq!(
        validate_override(PumpId::Makeup, Some(600)),
        Err(OverrideError::NotOverridable)
    );
    assert_eq!(
        validate_override(PumpId::Drain, None),
        Err(OverrideError::NotOverridable)
    );
}

#[test]
fn top_off_duration_is_bounded() {
    assert!(validate_top_off(0).is_err());
    assert!(validate_top_off(MAX_TOP_OFF_S + 1).is_err());
    assert_eq!(validate_top_off(120), Ok(()));
}

const SECOND: u64 = 1000;

fn plan(drain_s: u32, settle_s: u32, refill_s: u32) -> OperationPlan {
    OperationPlan::water_change(WaterChangePlan {
        drain_s,
        settle_s,
        refill_s,
    })
}

fn run(operation: &mut Operation, seconds: u64, levels: LevelConditions) -> Step {
    let mut last = Step::Continue;
    for _ in 0..seconds {
        last = operation.advance(SECOND, Some(levels));
        if last != Step::Continue {
            return last;
        }
    }
    last
}

#[test]
fn a_top_off_is_a_single_refilling_phase() {
    let mut operation = Operation::start(OperationPlan::top_off(3), Some(100));

    assert_eq!(operation.phase, Phase::Refilling);
    assert_eq!(operation.remaining_s(), 3);
    assert_eq!(run(&mut operation, 2, HEALTHY), Step::Continue);
    assert_eq!(
        operation.advance(SECOND, Some(HEALTHY)),
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
        top_off.advance(0, Some(full)),
        Step::Finished(OperationResult::SumpFull)
    );

    let mut draining = Operation::start(plan(60, 0, 60), None);
    assert_eq!(draining.advance(SECOND, Some(full)), Step::Continue);
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
        assert_eq!(operation.advance(0, Some(full)), Step::Continue);
    }
    assert_eq!(operation.ran_s(Phase::Refilling), 0);

    assert_eq!(operation.advance(SECOND, Some(HEALTHY)), Step::Continue);
    assert_eq!(
        operation.advance(0, Some(full)),
        Step::Finished(OperationResult::SumpFull)
    );
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
    assert_eq!(operation.advance(SECOND, Some(low)), Step::PhaseChanged);
    assert_eq!(operation.phase, Phase::Settling);
    assert_eq!(operation.ran_s(Phase::Draining), 60);
}

#[test]
fn missing_levels_abort_any_phase() {
    let mut operation = Operation::start(plan(60, 30, 60), None);

    assert_eq!(
        operation.advance(SECOND, None),
        Step::Finished(OperationResult::Aborted)
    );
}

#[test]
fn remaining_time_rounds_up() {
    let mut operation = Operation::start(plan(10, 0, 10), None);
    operation.advance(1500, Some(HEALTHY));

    assert_eq!(operation.remaining_s(), 9);
}

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
    operation.advance(4200, Some(HEALTHY));
    let checkpoint = Checkpoint {
        operation,
        checkpoint_at: Some(50),
    };

    let bytes = serde_json::to_vec(&checkpoint).expect("encode");
    let decoded: Checkpoint = serde_json::from_slice(&bytes).expect("decode");

    assert_eq!(decoded, checkpoint);
}

#[test]
fn operation_states_follow_phases() {
    assert_eq!(
        OperationState::from(Phase::Draining),
        OperationState::Draining
    );
    assert_eq!(
        OperationState::from(Phase::Settling),
        OperationState::Settling
    );
    assert_eq!(
        OperationState::from(Phase::Refilling),
        OperationState::Refilling
    );
}

#[test]
fn only_the_normally_closed_circulation_channels_fail_safe_on() {
    assert!(PumpId::Return.fail_safe_on());
    assert!(PumpId::Uv.fail_safe_on());
    assert!(!PumpId::Makeup.fail_safe_on());
    assert!(!PumpId::Drain.fail_safe_on());
}
