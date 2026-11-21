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

fn ms(millis: u64) -> core::time::Duration {
    core::time::Duration::from_millis(millis)
}

fn running() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReturnLevel,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
    ])
}

fn without(base: &WaterLevelSnapshot, dry: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = base.clone();
    for role in dry {
        snapshot.sensors[role.index()].state = WaterLevelState::Dry;
    }
    snapshot
}

fn with(base: &WaterLevelSnapshot, wet: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = base.clone();
    for role in wet {
        snapshot.sensors[role.index()].state = WaterLevelState::Wet;
    }
    snapshot
}

const FLOOD: [WaterLevelRole; 3] = [
    WaterLevelRole::MainTank,
    WaterLevelRole::SumpMechanical,
    WaterLevelRole::SumpBiological,
];

#[test]
fn an_unplugged_harness_dries_both_sump_sensors_at_once() {
    let mut watch = HarnessWatch::default();
    assert!(!watch.update(Some(&running()), ms(0)));

    let unplugged = WaterLevelSnapshot::dry();
    assert!(watch.update(Some(&unplugged), ms(100)));
    assert!(watch.update(Some(&unplugged), ms(3_600_000)));
    assert!(watch.update(None, ms(1000)));

    let back = with(&unplugged, &[WaterLevelRole::SumpPump]);
    assert!(!watch.update(Some(&back), ms(100)));
    assert!(!watch.update(Some(&unplugged), ms(100)));
}

#[test]
fn the_return_level_sensor_alone_also_clears_an_unplugged_harness() {
    let mut watch = HarnessWatch::default();
    watch.update(Some(&running()), ms(0));
    assert!(watch.update(Some(&WaterLevelSnapshot::dry()), ms(100)));

    let back = with(
        &WaterLevelSnapshot::dry(),
        &[WaterLevelRole::SumpReturnLevel],
    );
    assert!(!watch.update(Some(&back), ms(100)));
}

#[test]
fn the_sump_sensors_drying_more_than_three_seconds_apart_are_real_water() {
    for (gap_ms, suspect) in [(2900, true), (3000, true), (3100, false), (30_000, false)] {
        let mut watch = HarnessWatch::default();
        watch.update(Some(&running()), ms(0));
        let lowered = without(&running(), &[WaterLevelRole::SumpReturnLevel]);
        assert!(!watch.update(Some(&lowered), ms(100)));
        assert!(!watch.update(Some(&lowered), ms(gap_ms)));

        let dry = without(&lowered, &[WaterLevelRole::SumpPump]);
        assert_eq!(watch.update(Some(&dry), ms(0)), suspect, "{gap_ms} ms");
    }
}

#[test]
fn the_window_is_measured_across_missing_snapshots() {
    let mut watch = HarnessWatch::default();
    watch.update(Some(&running()), ms(0));
    let lowered = without(&running(), &[WaterLevelRole::SumpReturnLevel]);
    watch.update(Some(&lowered), ms(100));
    watch.update(None, ms(4000));

    let dry = without(&lowered, &[WaterLevelRole::SumpPump]);
    assert!(!watch.update(Some(&dry), ms(100)));
}

#[test]
fn a_shorted_harness_wets_the_tank_and_both_flood_sensors_at_once() {
    let mut watch = HarnessWatch::default();
    watch.update(Some(&running()), ms(0));

    let shorted = with(&running(), &FLOOD);
    assert!(watch.update(Some(&shorted), ms(100)));
    assert!(watch.update(Some(&shorted), ms(3_600_000)));

    for role in FLOOD {
        let mut probe = HarnessWatch::default();
        probe.update(Some(&running()), ms(0));
        assert!(probe.update(Some(&shorted), ms(100)));
        assert!(
            !probe.update(Some(&without(&shorted, &[role])), ms(100)),
            "{role:?} reads dry again"
        );
    }
}

#[test]
fn the_flood_sensors_wetting_one_by_one_are_real_water() {
    let mut watch = HarnessWatch::default();
    watch.update(Some(&running()), ms(0));

    let two = with(&running(), &FLOOD[..2]);
    assert!(!watch.update(Some(&two), ms(100)));
    assert!(!watch.update(Some(&two), ms(3100)));
    assert!(!watch.update(Some(&with(&two, &FLOOD[2..])), ms(0)));
}

#[test]
fn the_first_snapshot_is_not_a_transition() {
    let mut dry = HarnessWatch::default();
    assert!(!dry.update(Some(&WaterLevelSnapshot::dry()), ms(0)));
    assert!(!dry.update(Some(&WaterLevelSnapshot::dry()), ms(100)));

    let mut wet = HarnessWatch::default();
    let shorted = with(&running(), &FLOOD);
    assert!(!wet.update(Some(&shorted), ms(0)));
    assert!(!wet.update(Some(&shorted), ms(100)));
}

#[test]
fn transitions_across_a_long_gap_in_the_level_data_are_not_simultaneous() {
    let mut watch = HarnessWatch::default();
    watch.update(Some(&running()), ms(0));
    watch.update(None, ms(1000));
    watch.update(None, ms(2100));

    assert!(!watch.update(Some(&WaterLevelSnapshot::dry()), ms(100)));
    assert!(!watch.update(Some(&WaterLevelSnapshot::dry()), ms(100)));

    let mut short_gap = HarnessWatch::default();
    short_gap.update(Some(&running()), ms(0));
    short_gap.update(None, ms(2900));
    assert!(short_gap.update(Some(&WaterLevelSnapshot::dry()), ms(100)));
}
