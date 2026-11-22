use std::sync::Arc;
use std::time::Duration;

use tokio::time::Instant;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Blocker, OperationKind, OperationResult, PumpId, PumpSettingsPatch, SkipReason,
};
use crate::ports::inbound::{PumpControl, StartError};
use crate::ports::outbound::PumpRecord;

const HOUR: u64 = 3600;

fn drain_briefly(service: &Service) {
    observe(service, Some(HEALTHY));
    service.record(PumpId::Drain, true, None, None);
    service.observe(Some(HEALTHY), Duration::from_millis(500), Instant::now());
}

fn finish_a_water_change(service: &Service) {
    observe(service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");
    drain_briefly(service);
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
}

fn cancel_a_water_change_before_it_drains(service: &Service) {
    observe(service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");
    observe(service, Some(HEALTHY));
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
}

#[test]
fn a_water_change_cancelled_before_draining_does_not_hold_back_the_next() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    cancel_a_water_change_before_it_drains(&service);
    assert_eq!(
        service
            .status()
            .last_water_change
            .map(|record| (record.result, record.drain_ran_s)),
        Some((OperationResult::Aborted, 0))
    );

    clock.advance(60);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn an_undrained_water_change_keeps_the_gap_of_the_drained_one_before() {
    let clock = TestClock::synced(EPOCH);
    let store = Arc::new(VolatilePumpStore::default());
    let service = service_on(&store, Arc::clone(&clock));
    finish_a_water_change(&service);
    assert_eq!(
        service
            .status()
            .last_water_change
            .map(|record| record.drain_ran_s),
        Some(0),
        "half a second of draining rounds down in the record but still counts"
    );

    clock.advance(13 * HOUR);
    cancel_a_water_change_before_it_drains(&service);
    service
        .apply_settings(PumpSettingsPatch {
            min_water_change_gap_h: Some(24),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");

    let reloaded = service_on(&store, Arc::clone(&clock));
    observe(&reloaded, Some(HEALTHY));
    assert_eq!(
        reloaded.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );

    clock.advance(11 * HOUR);
    assert_eq!(reloaded.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_water_change_waits_out_the_gap_after_the_previous_one() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    finish_a_water_change(&service);

    clock.advance(12 * HOUR - 1);
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
    assert_eq!(service.start_top_off(60), Ok(()));
    service.cancel(OperationKind::TopOff).expect("cancelled");

    clock.advance(1);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_zero_gap_disables_the_water_change_limit() {
    let (service, _store) = service_and_store();
    service
        .apply_settings(PumpSettingsPatch {
            min_water_change_gap_h: Some(0),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
    finish_a_water_change(&service);

    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn the_water_change_gap_needs_both_times() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    finish_a_water_change(&service);

    clock.set_epoch(None);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
    drain_briefly(&service);
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");

    clock.set_epoch(Some(EPOCH + 60));
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_water_change_recorded_in_the_future_is_too_recent() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    finish_a_water_change(&service);

    clock.set_epoch(Some(EPOCH - HOUR));
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn a_scheduled_water_change_too_soon_is_skipped() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.service
        .apply_settings(PumpSettingsPatch {
            water_change: Some(vec![crate::domain::pump::WaterChangeWindow {
                cron: "0 10 * * sun".into(),
                drain_s: 10,
                settle_s: 0,
                refill_s: 10,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    drain_briefly(&rig.service);
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.kind, OperationKind::WaterChange);
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::RecentWaterChange));
}

fn reloaded(rig: &Rig) -> Arc<Service> {
    let service = service_on(&rig.store, Arc::clone(&rig.clock));
    observe(&service, Some(HEALTHY));
    service
}

#[test]
fn a_cancel_right_after_the_drain_switched_on_still_counts_for_the_gap() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.now += Duration::from_millis(40);
    rig.tick(Some(&healthy));
    assert!(rig.pins()[PumpId::Drain.index()]);

    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    rig.clock.advance(60);

    assert_eq!(
        rig.service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
    assert_eq!(
        reloaded(&rig).start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn a_cancel_before_the_drain_ever_switched_on_does_not_count_for_the_gap() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Drain);
    rig.now += Duration::from_millis(40);
    rig.tick(Some(&healthy));

    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    *rig.outputs.failing.lock().unwrap() = None;
    rig.clock.advance(60);

    assert_eq!(reloaded(&rig).start_water_change(PLAN), Ok(()));
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    assert_eq!(rig.service.start_water_change(PLAN), Ok(()));
}

const OLD_DRAINED_HISTORY: &[u8] = br#"{"top_off":null,
    "water_change":{"kind":"water_change","started_at":1700000000,"drain_ran_s":3,
    "settle_ran_s":2,"refill_ran_s":4,"result":"completed","resumed":false}}"#;

#[test]
fn an_old_history_keeps_its_drain_date_when_an_unsynced_cancel_overwrites_it() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(PumpRecord::LastOperations, OLD_DRAINED_HISTORY.to_vec());
    let clock = TestClock::synced(EPOCH + 100);
    let service = service_on(&store, Arc::clone(&clock));
    observe(&service, Some(HEALTHY));
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );

    clock.set_epoch(None);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
    observe(&service, Some(HEALTHY));
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");

    clock.set_epoch(Some(EPOCH + 101));
    let reloaded = service_on(&store, Arc::clone(&clock));
    observe(&reloaded, Some(HEALTHY));
    assert_eq!(
        reloaded.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}
