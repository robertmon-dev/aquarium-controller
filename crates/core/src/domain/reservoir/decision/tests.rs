use crate::domain::pump::Supervision;
use crate::domain::reservoir::fixtures::half;
use crate::domain::reservoir::*;

fn input(demand: Demand) -> DecisionInput {
    DecisionInput {
        conditions: Some(half()),
        supervision: Supervision::Active,
        fault: false,
        outputs_uncertain: false,
        water_change_active: false,
        resting: [false; 2],
        demand,
        ro_valve_open_ms: 0,
    }
}

const RO: Demand = Demand {
    inlet: Some(Inlet::Ro),
    mixer: true,
};
const TAP: Demand = Demand {
    inlet: Some(Inlet::Tap),
    mixer: true,
};

fn on(decisions: [ChannelDecision; 4]) -> [bool; 4] {
    decisions.map(|decision| decision.on)
}

fn leaking() -> Option<ReservoirConditions> {
    Some(ReservoirConditions {
        leak: Some(true),
        ..half()
    })
}

fn full() -> Option<ReservoirConditions> {
    Some(ReservoirConditions {
        full: true,
        ..half()
    })
}

fn valves(demand: Demand) -> &'static [ReservoirChannel] {
    match demand.inlet {
        Some(Inlet::Ro) => &[ReservoirChannel::RoValve, ReservoirChannel::Booster],
        _ => &[ReservoirChannel::TapValve],
    }
}

#[test]
fn the_booster_follows_the_ro_valve_after_its_lead() {
    assert_eq!(on(decide(input(RO))), [true, false, false, true]);
    let led = DecisionInput {
        ro_valve_open_ms: BOOSTER_LEAD_MS,
        ..input(RO)
    };
    assert_eq!(on(decide(led)), [true, true, false, true]);
}

#[test]
fn global_blockers_stop_every_channel_in_priority_order() {
    let cases = [
        (
            DecisionInput {
                fault: true,
                conditions: leaking(),
                supervision: Supervision::Unavailable,
                ..input(RO)
            },
            ReservoirBlocker::Fault,
        ),
        (
            DecisionInput {
                fault: true,
                conditions: None,
                supervision: Supervision::Off,
                ..input(RO)
            },
            ReservoirBlocker::Fault,
        ),
        (
            DecisionInput {
                conditions: leaking(),
                supervision: Supervision::Unavailable,
                ..input(RO)
            },
            ReservoirBlocker::Leak,
        ),
        (
            DecisionInput {
                supervision: Supervision::Unavailable,
                ..input(RO)
            },
            ReservoirBlocker::SupervisionUnavailable,
        ),
        (
            DecisionInput {
                supervision: Supervision::Off,
                conditions: None,
                ..input(RO)
            },
            ReservoirBlocker::SupervisionUnavailable,
        ),
        (
            DecisionInput {
                conditions: None,
                ..input(RO)
            },
            ReservoirBlocker::LevelsUnavailable,
        ),
    ];

    for (decision_input, blocker) in cases {
        assert_eq!(global_blocker(&decision_input), Some(blocker));
        let decisions = decide(decision_input);
        assert_eq!(on(decisions), [false; 4]);
        assert!(decisions
            .iter()
            .all(|decision| decision.blocked_by == Some(blocker)));
    }
    assert_eq!(global_blocker(&input(RO)), None);
}

#[test]
fn inlet_blockers_close_the_valves_but_keep_the_mixer() {
    for demand in [RO, TAP] {
        let cases = [
            (
                DecisionInput {
                    outputs_uncertain: true,
                    conditions: full(),
                    water_change_active: true,
                    resting: [true; 2],
                    ..input(demand)
                },
                ReservoirBlocker::Interlock,
            ),
            (
                DecisionInput {
                    conditions: full(),
                    water_change_active: true,
                    resting: [true; 2],
                    ..input(demand)
                },
                ReservoirBlocker::ReservoirFull,
            ),
            (
                DecisionInput {
                    water_change_active: true,
                    resting: [true; 2],
                    ..input(demand)
                },
                ReservoirBlocker::WaterChange,
            ),
            (
                DecisionInput {
                    resting: [true; 2],
                    ..input(demand)
                },
                ReservoirBlocker::DutyRest,
            ),
        ];

        for (decision_input, blocker) in cases {
            let decisions = decide(decision_input);
            assert_eq!(on(decisions), [false, false, false, true]);
            for channel in valves(demand) {
                assert_eq!(decisions[channel.index()].blocked_by, Some(blocker));
            }
            assert_eq!(
                inlet_blocker(
                    &decision_input,
                    decision_input.conditions.expect("conditions"),
                    demand.inlet.expect("inlet")
                ),
                Some(blocker)
            );
        }
    }
}

#[test]
fn a_water_change_closes_the_tap_valve_too() {
    let decisions = decide(DecisionInput {
        water_change_active: true,
        ..input(TAP)
    });
    assert_eq!(on(decisions), [false, false, false, true]);
    assert_eq!(
        decisions[ReservoirChannel::TapValve.index()].blocked_by,
        Some(ReservoirBlocker::WaterChange)
    );
}

#[test]
fn duty_rest_is_per_inlet() {
    let decisions = decide(DecisionInput {
        resting: [true, false],
        ..input(TAP)
    });
    assert_eq!(on(decisions), [false, false, true, true]);
    assert_eq!(
        decisions[ReservoirChannel::RoValve.index()].blocked_by,
        Some(ReservoirBlocker::DutyRest)
    );
    assert_eq!(
        decisions[ReservoirChannel::TapValve.index()].blocked_by,
        None
    );
}

#[test]
fn idle_channels_report_the_blocker_that_applies_to_them() {
    let decisions = decide(DecisionInput {
        conditions: full(),
        demand: Demand::default(),
        ..input(RO)
    });
    assert_eq!(on(decisions), [false; 4]);
    for channel in [
        ReservoirChannel::RoValve,
        ReservoirChannel::Booster,
        ReservoirChannel::TapValve,
    ] {
        assert_eq!(
            decisions[channel.index()].blocked_by,
            Some(ReservoirBlocker::ReservoirFull)
        );
    }
    assert_eq!(decisions[ReservoirChannel::Mixer.index()].blocked_by, None);
}

#[test]
fn the_mixer_never_runs_in_an_empty_barrel() {
    let empty = ReservoirConditions {
        empty: true,
        ..half()
    };
    let decisions = decide(DecisionInput {
        conditions: Some(empty),
        ..input(RO)
    });
    assert!(decisions[ReservoirChannel::RoValve.index()].on);
    assert_eq!(
        decisions[ReservoirChannel::Mixer.index()].blocked_by,
        Some(ReservoirBlocker::ReservoirEmpty)
    );
}
