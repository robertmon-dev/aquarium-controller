use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::application::reservoir::fixtures::*;
use crate::domain::reservoir::{
    FillFault, FillResult, OverrideTarget, ReservoirBlocker, ReservoirSettings, SkipReason,
};
use crate::ports::inbound::{FillStartError, ReservoirControl, ReservoirOverrideError};
use crate::ports::outbound::ReservoirRecord;

#[test]
fn overtime_stops_the_fill_and_keeps_a_fault_across_a_reboot() {
    let settings = ReservoirSettings {
        empty_fill_s: 600,
        max_duration_s: 43_200,
        ..RO_ONLY
    };
    let mut rig = Rig::new(settings);
    rig.start_fill().expect("started");
    rig.run(725);

    assert_eq!(rig.relays.pins(), [false; 4]);
    let status = rig.status();
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::Overtime)
    );
    assert_eq!(
        status.fault.map(|record| record.fault),
        Some(FillFault::Overtime)
    );

    let mut rebooted = rig.reboot();
    assert!(rebooted.status().fault.is_some());
    assert_eq!(
        rebooted.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::Fault))
    );
    assert_eq!(
        rebooted.set_override(OverrideTarget::Mixer, true, Some(10)),
        Err(ReservoirOverrideError::Blocked(ReservoirBlocker::Fault))
    );

    rebooted.service.clear_fault().expect("cleared");
    rebooted.run(1);
    assert_eq!(rebooted.start_fill(), Ok(()));
}

#[test]
fn a_reboot_mid_fill_holds_automation_until_a_manual_start() {
    let auto = ReservoirSettings {
        auto_fill: true,
        ..RO_ONLY
    };
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(10);

    let mut rebooted = Rig::build(Arc::clone(&rig.store), Arc::clone(&rig.clock), auto);
    let status = rebooted.status();
    assert!(status.held_after_reboot);
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::InterruptedByReboot)
    );

    rebooted.run(700);
    assert!(rebooted.status().fill.is_none());
    assert_eq!(
        rebooted.status().last_skipped.map(|skip| skip.reason),
        Some(SkipReason::HeldAfterReboot)
    );

    rebooted.start_fill().expect("manual start");
    assert!(!rebooted.status().held_after_reboot);
}

#[test]
fn an_unstorable_marker_keeps_every_valve_closed() {
    let mut rig = Rig::new(RO_ONLY);
    rig.store.refuse_marker.store(true, Ordering::Relaxed);

    assert!(matches!(
        rig.start_fill(),
        Err(FillStartError::NotPersisted(_))
    ));
    rig.run(5);
    assert_eq!(rig.relays.pins(), [false; 4]);

    let auto = ReservoirSettings {
        auto_fill: true,
        ..RO_ONLY
    };
    let mut automatic = Rig::build(Arc::clone(&rig.store), TestClock::synced(), auto);
    automatic.run(605);
    assert_eq!(automatic.relays.pins(), [false; 4]);
    assert_eq!(
        automatic.status().last_skipped.map(|skip| skip.reason),
        Some(SkipReason::Storage)
    );
}

#[test]
fn the_reboot_hold_survives_further_reboots() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(10);

    let first = rig.reboot();
    assert!(first.status().held_after_reboot);
    assert!(rig.store.inner.get(ReservoirRecord::Running).is_some());
    let recorded = rig.store.inner.get(ReservoirRecord::Last);

    let second = first.reboot();
    let status = second.status();
    assert!(status.held_after_reboot);
    assert!(!status.invalidation_pending);
    assert_eq!(rig.store.inner.get(ReservoirRecord::Last), recorded);
}
