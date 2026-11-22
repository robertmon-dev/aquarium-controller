use super::*;
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    reservoir_ready: true,
    tank_overflow: false,
    sump_full: false,
    sump_high: false,
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
            sump_high: false,
        }
    );
}

#[test]
fn only_the_ready_sensor_says_the_reservoir_holds_a_water_change() {
    let ready = snapshot(
        &[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady],
        &[],
    );
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
fn lost_supervision_keeps_the_makeup_off_while_refilling() {
    let decisions = decide(DecisionInput {
        phase: Some(Phase::Refilling),
        supervision_unavailable: true,
        ..input(Some(HEALTHY))
    });

    assert!(!decisions[2].on);
    assert_eq!(
        decisions[2].blocked_by,
        Some(Blocker::SupervisionUnavailable)
    );
    assert!(decisions[0].on);
    assert!(decisions[1].on);
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
fn the_reservoir_volume_is_optional_and_bounded() {
    assert_eq!(defaults().reservoir_ready_l, None);

    for litres in [1, MAX_RESERVOIR_L] {
        let mut settings = defaults();
        settings.reservoir_ready_l = Some(litres);
        assert_eq!(settings.validate(), Ok(()), "{litres}");
    }

    for litres in [0, MAX_RESERVOIR_L + 1] {
        let mut settings = defaults();
        settings.reservoir_ready_l = Some(litres);
        assert_eq!(
            settings.validate(),
            Err("reservoir_ready_l must be between 1 and 10000"),
            "{litres}"
        );
    }
}

fn reservoir_settings(drain_lph: Option<u32>, reservoir_l: Option<u32>) -> PumpSettings {
    let mut settings = defaults();
    settings.flow_lph.drain = drain_lph;
    settings.reservoir_ready_l = reservoir_l;
    settings
}

#[test]
fn a_drain_must_fit_in_the_ready_reservoir_when_both_are_known() {
    let settings = reservoir_settings(Some(1500), Some(50));

    assert!(settings.drain_fits_reservoir(120));
    assert!(!settings.drain_fits_reservoir(121));
    assert!(reservoir_settings(None, Some(50)).drain_fits_reservoir(MAX_DRAIN_S));
    assert!(reservoir_settings(Some(1500), None).drain_fits_reservoir(MAX_DRAIN_S));
}

#[test]
fn a_water_change_window_must_fit_in_the_ready_reservoir() {
    let window = |drain_s| WaterChangeWindow {
        drain_s,
        ..water_change("0 10 * * sun")
    };

    let mut fits = reservoir_settings(Some(1500), Some(50));
    fits.water_change = vec![window(120)];
    assert_eq!(fits.validate(), Ok(()));

    let mut too_long = reservoir_settings(Some(1500), Some(50));
    too_long.water_change = vec![window(120), window(121)];
    assert_eq!(
        too_long.validate(),
        Err("a water-change window drains more than reservoir_ready_l holds")
    );

    for unchecked in [
        reservoir_settings(None, Some(50)),
        reservoir_settings(Some(1500), None),
    ] {
        let mut settings = unchecked;
        settings.water_change = vec![window(MAX_DRAIN_S)];
        assert_eq!(settings.validate(), Ok(()));
    }
}

#[test]
fn a_patch_sets_keeps_or_clears_the_reservoir_volume() {
    let mut base = defaults();
    base.reservoir_ready_l = Some(40);

    let set: PumpSettingsPatch =
        serde_json::from_str(r#"{"reservoir_ready_l":47}"#).expect("patch");
    assert_eq!(set.merge_onto(&base).reservoir_ready_l, Some(47));

    let kept: PumpSettingsPatch = serde_json::from_str("{}").expect("patch");
    assert_eq!(kept.merge_onto(&base).reservoir_ready_l, Some(40));

    let cleared: PumpSettingsPatch =
        serde_json::from_str(r#"{"reservoir_ready_l":null}"#).expect("patch");
    assert_eq!(cleared.merge_onto(&base).reservoir_ready_l, None);
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
    assert_eq!(stored.reservoir_ready_l, None);
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
        last = operation.advance(SECOND, SECOND, Some(levels));
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
        operation.advance(0, 500 * SECOND, Some(HEALTHY)),
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

#[test]
fn level_conditions_distrust_an_implausible_return_chamber() {
    let stuck = snapshot(
        &[
            WaterLevelRole::SumpReturnLevel,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::ReservoirReady,
        ],
        &[],
    );

    let levels = LevelConditions::from_snapshot(&stuck, Some(WaterLevelRole::SumpReturnLevel));

    assert!(levels.sump_low);
    assert!(levels.sump_full);
    assert!(!levels.reservoir_empty && levels.reservoir_ready);
}

#[test]
fn level_conditions_distrust_an_implausible_reservoir() {
    let ready_over_empty = snapshot(
        &[WaterLevelRole::SumpPump, WaterLevelRole::ReservoirReady],
        &[],
    );
    let levels = LevelConditions::from_snapshot(&ready_over_empty, None);
    assert!(levels.reservoir_empty);
    assert!(!levels.reservoir_ready);

    let full_over_low = snapshot(
        &[
            WaterLevelRole::SumpPump,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::ReservoirFull,
        ],
        &[],
    );
    let levels = LevelConditions::from_snapshot(&full_over_low, None);
    assert!(!levels.reservoir_empty);
    assert!(!levels.reservoir_ready);
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
fn the_flood_sensors_report_a_high_sump() {
    for (wet, faulty) in [
        (vec![WaterLevelRole::SumpMechanical], vec![]),
        (vec![WaterLevelRole::SumpBiological], vec![]),
        (vec![], vec![WaterLevelRole::SumpMechanical]),
        (vec![], vec![WaterLevelRole::SumpBiological]),
    ] {
        let mut roles = vec![
            WaterLevelRole::SumpPump,
            WaterLevelRole::SumpReserve,
            WaterLevelRole::ReservoirReady,
        ];
        roles.extend(wet.iter().copied());
        let levels = LevelConditions::from_snapshot(
            &snapshot(&roles, &faulty),
            Some(WaterLevelRole::SumpReturnLevel),
        );
        assert_eq!(
            levels,
            LevelConditions {
                sump_high: true,
                ..HEALTHY
            },
            "wet {wet:?} faulty {faulty:?}"
        );
    }
}

#[test]
fn a_high_sump_blocks_the_makeup_after_a_full_one() {
    let refilling = DecisionInput {
        phase: Some(Phase::Refilling),
        ..input(Some(HEALTHY))
    };

    let high = decide(DecisionInput {
        levels: Some(LevelConditions {
            sump_high: true,
            ..HEALTHY
        }),
        ..refilling
    });
    assert!(!high[2].on);
    assert_eq!(high[2].blocked_by, Some(Blocker::SumpHigh));
    assert!(high[0].on && high[1].on);

    let both = decide(DecisionInput {
        levels: Some(LevelConditions {
            sump_full: true,
            sump_high: true,
            ..HEALTHY
        }),
        ..refilling
    });
    assert_eq!(both[2].blocked_by, Some(Blocker::SumpFull));
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
fn a_high_sump_blocker_is_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_string(&Blocker::SumpHigh).expect("encode"),
        r#""sump_high""#
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
