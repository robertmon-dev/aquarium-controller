use crate::domain::pump::fixtures::HEALTHY;
use crate::domain::pump::*;

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
fn an_unrecorded_drain_stays_off_until_its_checkpoint_is_stored() {
    let draining = DecisionInput {
        phase: Some(Phase::Draining),
        ..input(Some(HEALTHY))
    };

    let unrecorded = decide(DecisionInput {
        drain_unrecorded: true,
        ..draining
    });
    assert!(!unrecorded[3].on);
    assert_eq!(unrecorded[3].blocked_by, Some(Blocker::StorageUnavailable));

    assert!(decide(draining)[3].on);
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
fn a_high_sump_blocker_is_snake_case_on_the_wire() {
    assert_eq!(
        serde_json::to_string(&Blocker::SumpHigh).expect("encode"),
        r#""sump_high""#
    );
}
