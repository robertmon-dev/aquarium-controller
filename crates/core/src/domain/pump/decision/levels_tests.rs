use crate::domain::pump::fixtures::HEALTHY;
use crate::domain::pump::*;
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

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
