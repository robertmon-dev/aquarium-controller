use std::time::Duration;

use crate::application::pump::fixtures::*;
use crate::domain::cron::CivilTime;
use crate::domain::pump::{
    Blocker, OperationKind, OperationState, OperationStatus, PumpId, PumpSettingsPatch, SkipReason,
    SkippedWindow, Supervision, TopOffWindow,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot};
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::Clock;

fn schedule_top_off(service: &Service) {
    schedule_top_off_on(service);
}

#[test]
fn a_scheduled_window_fires_once_in_its_minute() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.run(1, Some(&healthy));
    assert!(!rig.pins()[2]);
}

#[test]
fn a_window_during_another_operation_is_skipped_as_busy() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    assert_eq!(
        rig.service.status().last_skipped,
        Some(SkippedWindow {
            kind: OperationKind::TopOff,
            index: 0,
            reason: SkipReason::Busy,
            blocked_by: None,
            at: Some(EPOCH + 1),
        })
    );
}

#[test]
fn a_blocked_window_is_skipped_with_its_blocker() {
    let mut rig = Rig::new();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);
    schedule_top_off(&rig.service);

    rig.tick(Some(&empty_reservoir));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&empty_reservoir));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::ReservoirEmpty));
}

#[test]
fn a_window_due_on_the_first_tick_sees_the_current_levels() {
    let mut rig = Rig::new();
    schedule_top_off(&rig.service);
    rig.clock.set_civil(Some(sunday_ten_am()));

    rig.tick(Some(&healthy()));

    assert!(rig.pins()[2]);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn a_scheduled_top_off_is_skipped_while_the_return_pump_is_forced_off() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::ReturnOff));
}

#[test]
fn a_scheduled_water_change_is_skipped_while_the_reservoir_is_low() {
    let mut rig = Rig::new();
    let reservoir_low = snapshot(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]);
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

    rig.tick(Some(&reservoir_low));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&reservoir_low));

    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Idle);
    let skipped = status.last_skipped.expect("skipped");
    assert_eq!(skipped.kind, OperationKind::WaterChange);
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::ReservoirLow));
    assert_eq!(rig.pins(), [true, true, false, false]);
}

fn fall_back_twice(rig: &mut Rig, step_back_s: u64) {
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.clock.set_civil(None);
    rig.run(120, Some(&healthy));

    let now = rig.clock.epoch_seconds().expect("synced");
    rig.clock.set_epoch(Some(now - step_back_s));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
}

#[test]
fn a_window_does_not_fire_twice_for_the_same_local_minute() {
    let mut rig = Rig::new();
    fall_back_twice(&mut rig, 0);

    assert!(!rig.pins()[2]);
    assert_eq!(rig.service.status().operation, OperationStatus::IDLE);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn a_backward_clock_step_does_not_refire_a_window() {
    let mut rig = Rig::new();
    fall_back_twice(&mut rig, 300);

    assert!(!rig.pins()[2]);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn the_same_local_minute_fires_again_a_day_later() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.clock.set_civil(None);

    let now = rig.clock.epoch_seconds().expect("synced");
    rig.clock.set_epoch(Some(now + 7 * 86_400));
    rig.clock.set_civil(Some(CivilTime {
        day: 14,
        ..sunday_ten_am()
    }));
    rig.run(1, Some(&healthy));

    assert!(rig.pins()[2]);
}

#[test]
fn a_skipped_window_is_not_retried_in_the_repeated_hour() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);
    schedule_top_off(&rig.service);

    rig.tick(Some(&empty_reservoir));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&empty_reservoir));
    let skipped = rig.service.status().last_skipped.expect("skipped");
    rig.clock.set_civil(None);
    rig.run(3600, Some(&healthy));

    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    assert!(!rig.pins()[2]);
    assert_eq!(rig.service.status().last_skipped, Some(skipped));
}

fn schedule_two_minutes(service: &Service) {
    service
        .apply_settings(PumpSettingsPatch {
            top_off: Some(vec![TopOffWindow {
                cron: "0,30 10 * * *".into(),
                duration_s: 120,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

fn fire_at(rig: &mut Rig, epoch: u64, minute: u8, levels: &WaterLevelSnapshot) -> bool {
    rig.clock.set_epoch(Some(epoch));
    rig.clock.set_civil(Some(CivilTime {
        minute,
        ..sunday_ten_am()
    }));
    rig.now += Duration::from_secs(1);
    rig.tick(Some(levels));
    let started = rig.pins()[PumpId::Makeup.index()];
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.clock.set_civil(None);
    rig.run(1, Some(levels));
    started
}

#[test]
fn a_window_matching_two_minutes_fires_each_once_across_a_fall_back() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_two_minutes(&rig.service);
    rig.tick(Some(&healthy));

    let fired: Vec<bool> = [(0, 0), (1800, 30), (3600, 0), (5400, 30)]
        .into_iter()
        .map(|(offset, minute)| fire_at(&mut rig, EPOCH + offset, minute, &healthy))
        .collect();

    assert_eq!(fired, [true, true, false, false]);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn a_minute_blocked_at_its_first_occurrence_is_not_retried_on_its_repeat() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);
    schedule_two_minutes(&rig.service);
    rig.tick(Some(&healthy));

    assert!(!fire_at(&mut rig, EPOCH, 0, &empty_reservoir));
    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert!(fire_at(&mut rig, EPOCH + 1800, 30, &healthy));
    assert!(!fire_at(&mut rig, EPOCH + 3600, 0, &healthy));
    assert!(!fire_at(&mut rig, EPOCH + 5400, 30, &healthy));

    assert_eq!(rig.service.status().last_skipped, Some(skipped));
}

#[test]
fn every_minute_of_a_dense_window_fires_only_once_across_a_fall_back() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.service
        .apply_settings(PumpSettingsPatch {
            top_off: Some(vec![TopOffWindow {
                cron: "* 10 * * *".into(),
                duration_s: 120,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
    rig.tick(Some(&healthy));

    for minute in 0..60u8 {
        assert!(fire_at(
            &mut rig,
            EPOCH + u64::from(minute) * 60,
            minute,
            &healthy
        ));
    }

    for minute in 0..60u8 {
        assert!(!fire_at(
            &mut rig,
            EPOCH + 3600 + u64::from(minute) * 60,
            minute,
            &healthy
        ));
    }
}

#[test]
fn a_scheduled_window_without_supervision_is_skipped_with_that_reason() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_two_minutes(&rig.service);
    rig.tick(Some(&healthy));
    rig.service.set_supervision(Supervision::Unavailable);

    assert!(!fire_at(&mut rig, EPOCH, 0, &healthy));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::SupervisionUnavailable));
}
