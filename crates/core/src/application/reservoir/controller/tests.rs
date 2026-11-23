use crate::application::reservoir::fixtures::*;
use crate::domain::reservoir::{FillResult, OverrideTarget, ReservoirBlocker, ReservoirChannel};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot};
use crate::ports::inbound::ReservoirControl;
use crate::ports::outbound::ReservoirRecord;

use ReservoirChannel::{Booster, Mixer, RoValve, TapValve};

#[test]
fn a_manual_fill_opens_the_ro_valve_and_then_the_booster() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");

    rig.run(1);
    assert!(rig.relays.pin(RoValve));
    assert!(!rig.relays.pin(Booster));

    rig.run(2);
    assert!(rig.relays.pin(Booster));
    assert!(rig.relays.pin(Mixer));
    assert!(!rig.relays.pin(TapValve));
}

#[test]
fn the_tap_valve_waits_for_a_successful_ro_off() {
    let mut rig = Rig::new(RO_ONLY);
    rig.set_override(OverrideTarget::Ro, true, Some(60))
        .expect("ro on");
    rig.run(3);
    assert!(rig.relays.pin(RoValve));

    rig.relays.refuse_off(Some(RoValve));
    rig.service.clear_override(OverrideTarget::Ro);
    rig.set_override(OverrideTarget::Tap, true, Some(60))
        .expect("tap accepted");
    rig.run(3);

    assert!(rig.relays.pin(RoValve));
    assert!(!rig.relays.pin(TapValve));
    let status = rig.status();
    assert!(status.channels.ro_valve.output_error);
    assert_eq!(
        status.channels.tap_valve.blocked_by,
        Some(ReservoirBlocker::Interlock)
    );

    rig.relays.refuse_off(None);
    rig.run(2);
    assert!(!rig.relays.pin(RoValve));
    assert!(rig.relays.pin(TapValve));
}

#[test]
fn a_booster_stuck_on_still_closes_the_ro_valve_and_ends_the_fill() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);
    assert!(rig.relays.pin(Booster));

    rig.relays.refuse_off(Some(Booster));
    rig.water_change = true;
    rig.run(2);

    assert!(!rig.relays.pin(RoValve));
    assert!(rig.relays.pin(Booster));
    assert_eq!(
        rig.status().last.map(|record| record.result),
        Some(FillResult::Blocked)
    );
}

#[test]
fn a_water_change_closes_both_inlets_and_keeps_the_mixer() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);

    rig.water_change = true;
    rig.run(3);
    assert_eq!(rig.relays.pins(), [false, false, false, true]);
    let status = rig.status();
    assert_eq!(
        status.channels.ro_valve.blocked_by,
        Some(ReservoirBlocker::WaterChange)
    );
    assert!(status.fill.is_some());

    rig.water_change = false;
    rig.run(4);
    assert!(rig.relays.pin(RoValve));
    assert!(rig.relays.pin(Booster));
}

#[test]
fn a_failed_on_write_ends_the_fill_as_blocked() {
    let mut rig = Rig::new(RO_ONLY);
    rig.relays.refuse_on(Some(RoValve));
    rig.start_fill().expect("started");

    rig.run(1);
    assert!(!rig.relays.pin(RoValve));
    assert!(rig.status().channels.ro_valve.output_error);

    rig.run(1);
    assert_eq!(rig.relays.pins(), [false; 4]);
    assert_eq!(
        rig.status().last.map(|record| record.result),
        Some(FillResult::Blocked)
    );
}

#[test]
fn an_unplugged_harness_counts_as_missing_levels() {
    let mut rig = Rig::new(RO_ONLY);
    rig.levels = barrel(&[
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReturnLevel,
    ]);
    rig.start_fill().expect("started");
    rig.run(5);
    assert!(rig.relays.pin(RoValve));

    rig.levels = WaterLevelSnapshot::dry();
    rig.run(1);

    assert_eq!(rig.relays.pins(), [false; 4]);
    let status = rig.status();
    assert_eq!(
        status.channels.ro_valve.blocked_by,
        Some(ReservoirBlocker::LevelsUnavailable)
    );
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::Blocked)
    );
}

#[test]
fn runtime_is_stored_whenever_an_output_switches() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(11);
    rig.cancel_fill().expect("cancelled");
    rig.run(1);

    let stored: [u64; 4] = rig
        .store
        .decode(ReservoirRecord::Runtime)
        .expect("runtime stored");
    assert_eq!(stored[RoValve.index()], 11);
}

#[test]
fn a_refused_on_write_before_a_start_does_not_end_the_new_fill() {
    let mut rig = Rig::new(RO_ONLY);
    rig.relays.refuse_on(Some(TapValve));
    rig.set_override(OverrideTarget::Tap, true, Some(60))
        .expect("tap on");
    rig.run(1);
    assert!(rig.status().channels.tap_valve.output_error);

    rig.relays.refuse_on(None);
    rig.start_fill().expect("started");
    rig.run(2);

    assert!(rig.status().fill.is_some());
    assert!(rig.relays.pin(RoValve));
}
