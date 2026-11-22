use crate::domain::reservoir::{
    Fill, FillInput, FillPhase, FillPlan, FillStep, FillTrigger, ReservoirConditions,
    ReservoirSettings,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

pub(super) fn snapshot(wet: &[WaterLevelRole], faulty: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = WaterLevelSnapshot::dry();
    for reading in &mut snapshot.sensors {
        if wet.contains(&reading.role) {
            reading.state = WaterLevelState::Wet;
        }
        reading.faulty = faulty.contains(&reading.role);
    }
    snapshot
}

pub(super) const HALF: [WaterLevelRole; 2] =
    [WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady];
pub(super) const FULL: [WaterLevelRole; 3] = [
    WaterLevelRole::SumpReserve,
    WaterLevelRole::ReservoirReady,
    WaterLevelRole::ReservoirFull,
];

pub(super) fn half() -> ReservoirConditions {
    ReservoirConditions::from_snapshot(&snapshot(&HALF, &[]), None)
}

pub(super) const TARGET: ReservoirSettings = ReservoirSettings {
    target_tds_ppm: Some(150),
    tolerance_ppm: 10,
    mix_s: 60,
    tap_pulse_s: 5,
    ro_pulse_s: 120,
    ..ReservoirSettings::DEFAULT
};

pub(super) fn barrel(empty: bool, full: bool) -> ReservoirConditions {
    ReservoirConditions {
        empty,
        ready: !empty,
        full,
        full_faulty: false,
        leak: None,
    }
}

pub(super) fn plan() -> FillPlan {
    TARGET.fill_plan()
}

pub(super) fn step(
    fill: &mut Fill,
    conditions: ReservoirConditions,
    ms: u64,
    ran: [bool; 3],
    tds: Option<f32>,
) -> FillStep {
    fill.advance(FillInput {
        conditions,
        elapsed_ms: ms,
        delivered: [ran[0], ran[1]],
        mixed: ran[2],
        tds,
    })
}

pub(super) const IDLE: [bool; 3] = [false, false, false];
pub(super) const RO_ON: [bool; 3] = [true, false, false];
pub(super) const MIXING: [bool; 3] = [false, false, true];
pub(super) const TAP_MIXING: [bool; 3] = [false, true, true];

pub(super) fn measuring(plan: FillPlan) -> Fill {
    let mut fill = Fill::start(FillTrigger::Manual, None, plan);
    step(&mut fill, barrel(false, false), 0, IDLE, None);
    step(&mut fill, barrel(false, false), 60_000, MIXING, None);
    assert_eq!(fill.phase(), FillPhase::Measuring);
    fill
}
