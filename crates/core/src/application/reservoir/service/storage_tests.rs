use std::sync::atomic::Ordering;
use std::sync::Arc;

use crate::application::reservoir::fixtures::*;
use crate::domain::reservoir::{
    FaultRecord, FillFault, FillResult, FillTrigger, ReservoirBlocker, ReservoirSettings,
    SkipReason,
};
use crate::ports::inbound::{FillCancelError, FillStartError, ReservoirControl};
use crate::ports::outbound::{ReservoirRecord, ReservoirStore};

#[test]
fn a_fault_that_cannot_be_stored_keeps_the_marker_and_is_retried() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);
    rig.store.refuse_fault.store(true, Ordering::Relaxed);

    rig.leak = Some(true);
    rig.run(2);

    let status = rig.status();
    assert_eq!(
        status.fault.map(|record| record.fault),
        Some(FillFault::Leak)
    );
    assert!(status.invalidation_pending);
    assert!(rig.store.inner.get(ReservoirRecord::Running).is_some());
    assert_eq!(rig.store.inner.get(ReservoirRecord::Fault), None);

    let rebooted = rig.reboot();
    assert_eq!(
        rebooted.status().fault.map(|record| record.fault),
        Some(FillFault::Leak)
    );
    assert_eq!(
        rebooted.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::Fault))
    );

    rig.store.refuse_fault.store(false, Ordering::Relaxed);
    rig.run(1);
    let stored: Option<FaultRecord> = rig
        .store
        .decode(ReservoirRecord::Fault)
        .expect("fault stored");
    assert_eq!(stored.map(|record| record.fault), Some(FillFault::Leak));
    assert_eq!(rig.store.inner.get(ReservoirRecord::Running), None);
    assert!(!rig.status().invalidation_pending);
}

#[test]
fn a_marker_that_cannot_be_erased_is_reported_and_retried() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);
    rig.store.refuse_erase.store(true, Ordering::Relaxed);

    assert!(matches!(
        rig.cancel_fill(),
        Err(FillCancelError::NotPersisted(_))
    ));
    rig.run(1);
    assert!(rig.status().invalidation_pending);
    assert!(matches!(
        rig.cancel_fill(),
        Err(FillCancelError::NotPersisted(_))
    ));

    rig.store.refuse_erase.store(false, Ordering::Relaxed);
    rig.run(1);
    assert!(!rig.status().invalidation_pending);
    assert_eq!(rig.store.inner.get(ReservoirRecord::Running), None);
    assert_eq!(rig.cancel_fill(), Ok(()));
}

#[test]
fn a_recorded_finish_with_a_leftover_marker_is_not_an_interruption() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(5);
    rig.store.refuse_erase.store(true, Ordering::Relaxed);
    let _ = rig.cancel_fill();

    let mut rebooted = rig.reboot();
    let status = rebooted.status();
    assert!(!status.held_after_reboot);
    assert!(status.invalidation_pending);
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::Aborted)
    );

    rig.store.refuse_erase.store(false, Ordering::Relaxed);
    rebooted.run(1);
    assert!(!rebooted.status().invalidation_pending);
    assert_eq!(rig.store.inner.get(ReservoirRecord::Running), None);
}

#[test]
fn an_unreadable_fault_record_locks_the_barrel_until_cleared() {
    let rig = Rig::new(RO_ONLY);
    rig.store.corrupt(ReservoirRecord::Fault);

    let mut rebooted = rig.reboot();
    assert_eq!(
        rebooted.status().fault,
        Some(FaultRecord {
            fault: FillFault::Unreadable,
            at: None,
        })
    );
    assert_eq!(
        rebooted.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::Fault))
    );

    rebooted.service.clear_fault().expect("cleared");
    assert_eq!(
        rebooted
            .store
            .decode::<Option<FaultRecord>>(ReservoirRecord::Fault),
        Some(None)
    );
    rebooted.run(1);
    assert_eq!(rebooted.start_fill(), Ok(()));
}

#[test]
fn an_unreadable_marker_holds_automation_like_an_interrupted_fill() {
    let auto = ReservoirSettings {
        auto_fill: true,
        ..RO_ONLY
    };
    let store = Arc::<FlakyStore>::default();
    store.corrupt(ReservoirRecord::Running);

    let mut rig = Rig::build(store, TestClock::synced(), auto);
    let status = rig.status();
    assert!(status.held_after_reboot);
    let last = status.last.expect("recorded");
    assert_eq!(last.result, FillResult::InterruptedByReboot);
    assert_eq!(last.trigger, FillTrigger::Auto);
    assert_eq!(last.started_at, None);

    rig.run(700);
    assert!(rig.status().fill.is_none());
    assert_eq!(
        rig.status().last_skipped.map(|skip| skip.reason),
        Some(SkipReason::HeldAfterReboot)
    );

    rig.start_fill().expect("manual start");
    assert!(!rig.status().held_after_reboot);
}

#[test]
fn a_saturated_runtime_record_keeps_counting_without_overflow() {
    let store = Arc::<FlakyStore>::default();
    let saturated = serde_json::to_vec(&[u64::MAX, 0, 0, 0]).expect("json");
    store
        .inner
        .store(ReservoirRecord::Runtime, &saturated)
        .expect("volatile store accepts");

    let mut rig = Rig::build(store, TestClock::synced(), RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(3);

    assert_eq!(rig.status().channels.ro_valve.runtime_s, u64::MAX / 1000);
}
