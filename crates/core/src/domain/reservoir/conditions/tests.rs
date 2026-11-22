use crate::domain::reservoir::fixtures::{snapshot, FULL, HALF};
use crate::domain::reservoir::*;
use crate::domain::water::WaterLevelRole;

#[test]
fn conditions_read_only_the_barrel_sensors() {
    let empty = ReservoirConditions::from_snapshot(&snapshot(&[], &[]), None);
    assert!(empty.empty && !empty.ready && !empty.full);

    let half = ReservoirConditions::from_snapshot(&snapshot(&HALF, &[]), None);
    assert!(!half.empty && half.ready && !half.full);

    let full = ReservoirConditions::from_snapshot(&snapshot(&FULL, &[]), Some(false));
    assert!(full.full && !full.full_faulty && !full.leaking());
}

#[test]
fn a_faulty_full_sensor_counts_as_full_and_a_faulty_reserve_as_empty() {
    let conditions = ReservoirConditions::from_snapshot(
        &snapshot(
            &HALF,
            &[WaterLevelRole::ReservoirFull, WaterLevelRole::SumpReserve],
        ),
        None,
    );

    assert!(conditions.full);
    assert!(conditions.full_faulty);
    assert!(conditions.empty);
}

#[test]
fn only_a_wet_leak_sensor_leaks() {
    let leak = |value| ReservoirConditions::from_snapshot(&snapshot(&HALF, &[]), value).leaking();
    assert!(!leak(None));
    assert!(!leak(Some(false)));
    assert!(leak(Some(true)));
}
