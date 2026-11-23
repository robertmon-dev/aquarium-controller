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

#[test]
fn a_flash_read_error_on_the_fault_key_locks_the_inlets() {
    let store = Arc::<FlakyStore>::default();
    store.fail_fault_read.store(true, Ordering::Relaxed);
    let rig = Rig::build(store, TestClock::synced(), RO_ONLY);
    assert_eq!(rig.status().fault.unwrap().fault, FillFault::Unreadable);
    assert_eq!(
        rig.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::Fault))
    );
}

#[test]
fn the_marker_outlives_a_failed_result_write() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().unwrap();
    rig.run(5);
    rig.store.refuse_last.store(true, Ordering::Relaxed);
    assert!(matches!(
        rig.cancel_fill(),
        Err(FillCancelError::NotPersisted(_))
    ));
    rig.run(1);
    assert_eq!(rig.relays.pins(), [false; 4]);
    assert!(rig.store.inner.get(ReservoirRecord::Running).is_some());
    assert!(matches!(
        rig.start_fill(),
        Err(FillStartError::NotPersisted(_))
    ));

    rig.store.refuse_last.store(false, Ordering::Relaxed);
    rig.run(1);
    assert!(!rig.status().invalidation_pending);
    assert!(rig.store.inner.get(ReservoirRecord::Running).is_none());
    assert_eq!(
        rig.reboot().status().last.unwrap().result,
        FillResult::Aborted
    );
}

#[test]
fn an_unsynced_overtime_survives_a_failed_fault_write_and_reboot() {
    let settings = ReservoirSettings {
        empty_fill_s: 600,
        ..RO_ONLY
    };
    let mut rig = Rig::build(Arc::default(), Arc::default(), settings);
    rig.start_fill().unwrap();
    rig.store.refuse_fault.store(true, Ordering::Relaxed);
    rig.run(725);
    let last = rig.status().last.unwrap();
    assert_eq!(last.result, FillResult::Overtime);
    assert_eq!(last.started_at, None);
    assert_eq!(last.finished_at, None);
    rig.store.refuse_fault.store(false, Ordering::Relaxed);
    let reboot = rig.reboot();
    assert_eq!(reboot.status().last, Some(last));
    assert_eq!(reboot.status().fault.unwrap().fault, FillFault::Overtime);
    assert_eq!(
        reboot.start_fill(),
        Err(FillStartError::Blocked(ReservoirBlocker::Fault))
    );
    assert_eq!(reboot.reboot().status().fault, reboot.status().fault);
}

#[test]
fn cancelling_and_restarting_in_one_second_keeps_the_second_fills_restart_hold() {
    let rig = Rig::new(RO_ONLY);
    rig.start_fill().unwrap();
    rig.cancel_fill().unwrap();
    rig.start_fill().unwrap();
    let reboot = rig.reboot();
    assert!(reboot.status().held_after_reboot);
    assert!(reboot.reboot().status().held_after_reboot);
    assert_eq!(
        reboot.status().last.unwrap().result,
        FillResult::InterruptedByReboot
    );
}

#[test]
fn automatic_cancellation_and_both_release_paths_survive_reboot() {
    let settings = ReservoirSettings {
        auto_fill: true,
        refill_debounce_s: 60,
        ..RO_ONLY
    };
    let mut rig = Rig::new(settings);
    rig.run(61);
    rig.cancel_fill().unwrap();
    let mut reboot = rig.reboot();
    reboot.run(65);
    assert!(reboot.status().auto_latched);
    assert_eq!(reboot.relays.pins(), [false; 4]);
    assert!(reboot.status().fill.is_none());

    reboot.levels = barrel(&FULL);
    reboot.tick();
    let mut dry = reboot.reboot();
    assert!(!dry.status().auto_latched);
    dry.run(65);
    assert!(dry.status().fill.is_some());
    dry.cancel_fill().unwrap();
    let manual = dry.reboot();
    manual.start_fill().unwrap();
    manual.cancel_fill().unwrap();
    assert!(!manual.reboot().status().auto_latched);
}

#[test]
fn a_legacy_unsynced_fault_is_preserved_even_when_the_cycle_identity_is_ambiguous() {
    use crate::domain::reservoir::{FillRecord, RunningMarker};
    let rig = Rig::new(RO_ONLY);
    let marker = RunningMarker {
        trigger: FillTrigger::Manual,
        started_at: None,
    };
    let mut last = FillRecord::interrupted(marker);
    last.result = FillResult::Overtime;
    rig.store
        .store(ReservoirRecord::Last, &serde_json::to_vec(&last).unwrap())
        .unwrap();
    rig.store
        .store(
            ReservoirRecord::Running,
            &serde_json::to_vec(&marker).unwrap(),
        )
        .unwrap();
    let reboot = rig.reboot();
    assert_eq!(reboot.status().last, Some(last));
    assert!(reboot.status().held_after_reboot);
    assert_eq!(reboot.status().fault.unwrap().fault, FillFault::Overtime);
    reboot.service.clear_fault().unwrap();
    let cleared = reboot.reboot();
    assert!(cleared.status().fault.is_none());
    assert!(cleared.status().held_after_reboot);
    assert_eq!(cleared.status().last, Some(last));
    cleared.start_fill().unwrap();
}

#[test]
fn clearing_an_inherited_auto_latch_is_retried_until_durable() {
    use crate::domain::reservoir::{FillRecord, RunningMarker};
    let rig = Rig::new(RO_ONLY);
    let mut last = FillRecord::interrupted(RunningMarker {
        trigger: FillTrigger::Auto,
        started_at: Some(EPOCH),
    });
    last.result = FillResult::Timeout;
    rig.store
        .store(ReservoirRecord::Last, &serde_json::to_vec(&last).unwrap())
        .unwrap();
    let mut reboot = rig.reboot();
    assert!(reboot.status().auto_latched);
    reboot.store.refuse_last.store(true, Ordering::Relaxed);
    reboot.levels = barrel(&FULL);
    reboot.tick();
    assert!(reboot.reboot().status().auto_latched);
    reboot.store.refuse_last.store(false, Ordering::Relaxed);
    reboot.tick();
    let cleared = reboot.reboot();
    assert!(!cleared.status().auto_latched);
    assert_eq!(cleared.status().last, Some(last));
}
