use super::*;

fn snapshot(wet: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = WaterLevelSnapshot::dry();

    for reading in &mut snapshot.sensors {
        if wet.contains(&reading.role) {
            reading.state = WaterLevelState::Wet;
        }
    }

    snapshot
}

fn faulty(snapshot: &WaterLevelSnapshot) -> Vec<WaterLevelRole> {
    snapshot
        .sensors
        .iter()
        .filter(|reading| reading.faulty)
        .map(|reading| reading.role)
        .collect()
}

#[test]
fn consistent_levels_stay_healthy() {
    use WaterLevelRole::*;

    for wet in [
        vec![],
        vec![SumpPump],
        vec![SumpPump, SumpReturnLevel],
        vec![SumpReserve],
        vec![SumpReserve, ReservoirReady],
        vec![SumpReserve, ReservoirReady, ReservoirFull],
        WaterLevelRole::ALL.to_vec(),
    ] {
        let checked = snapshot(&wet).with_plausibility();
        assert!(checked.all_healthy(), "{wet:?}: {:?}", faulty(&checked));
    }
}

#[test]
fn a_wet_upper_sensor_over_a_dry_lower_one_flags_both() {
    use WaterLevelRole::*;

    for (upper, lower, others) in [
        (SumpReturnLevel, SumpPump, vec![SumpReserve, ReservoirReady]),
        (ReservoirFull, ReservoirReady, vec![SumpReserve, SumpPump]),
        (ReservoirReady, SumpReserve, vec![SumpPump]),
    ] {
        let mut wet = others.clone();
        wet.push(upper);

        let checked = snapshot(&wet).with_plausibility();

        assert_eq!(faulty(&checked), sorted(vec![upper, lower]), "{upper:?}");
        assert_eq!(checked.get(upper).state, WaterLevelState::Wet);
        assert_eq!(checked.get(lower).state, WaterLevelState::Dry);
    }
}

#[test]
fn only_the_contradicting_pair_is_flagged() {
    use WaterLevelRole::*;

    let checked = snapshot(&[ReservoirReady, ReservoirFull, SumpPump]).with_plausibility();

    assert_eq!(faulty(&checked), sorted(vec![SumpReserve, ReservoirReady]));
}

#[test]
fn the_sump_mechanical_and_biological_sensors_are_not_cross_checked() {
    use WaterLevelRole::*;

    let checked = snapshot(&[SumpMechanical, SumpBiological]).with_plausibility();

    assert!(checked.all_healthy());
}

#[test]
fn an_existing_fault_is_kept() {
    let mut raw = snapshot(&[WaterLevelRole::SumpPump]);
    raw.sensors[WaterLevelRole::MainTank.index()].faulty = true;

    let checked = raw.with_plausibility();

    assert_eq!(faulty(&checked), vec![WaterLevelRole::MainTank]);
}

fn sorted(mut roles: Vec<WaterLevelRole>) -> Vec<WaterLevelRole> {
    roles.sort_by_key(|role| role.index());
    roles
}
