use std::sync::atomic::Ordering;

use crate::application::reservoir::fixtures::*;
use crate::domain::pump::Supervision;
use crate::domain::reservoir::{
    FillFault, FillResult, FillTrigger, OverrideTarget, ReservoirBlocker, ReservoirChannel,
    ReservoirSettings, SkipReason,
};
use crate::ports::inbound::ReservoirControl;

const AUTO: ReservoirSettings = ReservoirSettings {
    auto_fill: true,
    ..RO_ONLY
};

#[test]
fn stale_levels_mid_fill_close_everything_and_end_it_blocked() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);

    rig.levels_fresh = false;
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
    assert_eq!(status.fault, None);
}

#[test]
fn losing_supervision_mid_fill_closes_everything_and_ends_it_blocked() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);

    rig.service.set_supervision(Supervision::Unavailable);
    rig.run(1);

    assert_eq!(rig.relays.pins(), [false; 4]);
    let status = rig.status();
    assert_eq!(
        status.channels.mixer.blocked_by,
        Some(ReservoirBlocker::SupervisionUnavailable)
    );
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::Blocked)
    );
}

#[test]
fn a_wet_leak_sensor_with_stale_levels_ends_the_fill_with_a_leak_fault() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);

    rig.levels_fresh = false;
    rig.leak = Some(true);
    rig.run(1);

    assert_eq!(rig.relays.pins(), [false; 4]);
    let status = rig.status();
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::Leak)
    );
    assert_eq!(
        status.fault.map(|record| record.fault),
        Some(FillFault::Leak)
    );
}

#[test]
fn a_wet_leak_sensor_while_idle_records_a_leak_fault() {
    let mut rig = Rig::new(RO_ONLY);
    rig.set_override(OverrideTarget::Mixer, true, Some(60))
        .expect("mixer on");
    rig.run(1);
    assert!(rig.relays.pin(ReservoirChannel::Mixer));

    rig.leak = Some(true);
    rig.run(1);
    assert_eq!(rig.relays.pins(), [false; 4]);
    assert_eq!(
        rig.status().fault.map(|record| record.fault),
        Some(FillFault::Leak)
    );
    assert_eq!(rig.status().last, None);

    rig.leak = Some(false);
    rig.run(1);
    rig.service.clear_fault().expect("cleared");

    rig.levels_fresh = false;
    rig.leak = Some(true);
    rig.run(1);
    assert_eq!(
        rig.status().fault.map(|record| record.fault),
        Some(FillFault::Leak)
    );
}

#[test]
fn cancelling_an_automatic_fill_latches_automation() {
    let mut rig = Rig::new(AUTO);
    rig.run(601);
    assert_eq!(
        rig.status().fill.map(|fill| fill.trigger),
        Some(FillTrigger::Auto)
    );

    rig.cancel_fill().expect("cancelled");
    assert!(rig.status().auto_latched);

    rig.run(700);
    assert!(rig.status().fill.is_none());
    assert_eq!(
        rig.status().last_skipped.map(|skip| skip.reason),
        Some(SkipReason::AutoLatched)
    );

    rig.start_fill().expect("manual start");
    assert!(!rig.status().auto_latched);
}

#[test]
fn a_refused_marker_is_retried_once_per_debounce_period() {
    let mut rig = Rig::new(AUTO);
    rig.store.refuse_marker.store(true, Ordering::Relaxed);

    rig.run(1199);
    assert_eq!(rig.store.marker_writes.load(Ordering::Relaxed), 1);
    rig.run(1);
    assert_eq!(rig.store.marker_writes.load(Ordering::Relaxed), 2);

    rig.store.refuse_marker.store(false, Ordering::Relaxed);
    rig.run(600);
    assert!(rig.status().fill.is_some());
    assert_eq!(rig.status().last_skipped, None);
}

#[test]
fn a_faulty_full_sensor_ends_an_active_fill_as_blocked() {
    use crate::domain::water::WaterLevelRole;
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().unwrap();
    rig.run(3);
    assert!(rig.relays.pin(ReservoirChannel::RoValve));
    rig.levels.sensors[WaterLevelRole::ReservoirFull.index()].faulty = true;
    rig.tick();
    assert_eq!(rig.relays.pins(), [false; 4]);
    assert_eq!(rig.status().last.unwrap().result, FillResult::Blocked);
}
