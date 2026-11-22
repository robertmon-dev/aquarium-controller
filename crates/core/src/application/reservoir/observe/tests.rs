use crate::application::reservoir::fixtures::*;
use crate::domain::reservoir::{
    FillPhase, FillResult, FillTrigger, Inlet, ReservoirBlocker, ReservoirChannel,
    ReservoirSettings, SkipReason,
};
use crate::ports::inbound::ReservoirControl;
use crate::ports::outbound::ReservoirRecord;

use ReservoirChannel::{Booster, RoValve, TapValve};

#[test]
fn reaching_full_closes_everything_and_records_the_fill() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(30);

    rig.levels = barrel(&FULL);
    rig.run(2);

    assert_eq!(rig.relays.pins(), [false; 4]);
    let last = rig.status().last.expect("recorded");
    assert_eq!(last.result, FillResult::Completed);
    assert!(last.ro_s >= 28);
    assert_eq!(rig.store.inner.get(ReservoirRecord::Running), None);
}

#[test]
fn a_targeted_fill_doses_tap_water_after_a_soft_reading() {
    let mut rig = Rig::new(TARGETED);
    rig.tds = Some(100.0);
    rig.tick();
    rig.start_fill().expect("started");

    rig.run(1);
    assert_eq!(
        rig.status().fill.map(|fill| fill.phase),
        Some(FillPhase::Mixing)
    );
    rig.run(62);
    assert_eq!(
        rig.status().fill.and_then(|fill| fill.dosing),
        Some(Inlet::Tap)
    );
    assert!(rig.relays.pin(TapValve));
    assert!(!rig.relays.pin(RoValve));
}

#[test]
fn a_long_ro_run_rests_and_then_resumes() {
    let settings = ReservoirSettings {
        inlet_max_on_s: 600,
        inlet_rest_s: 60,
        ..RO_ONLY
    };
    let mut rig = Rig::new(settings);
    rig.start_fill().expect("started");

    rig.run(602);
    assert!(!rig.relays.pin(RoValve));
    assert!(!rig.relays.pin(Booster));
    assert!(rig.status().rest_remaining_s.ro.is_some());
    assert_eq!(
        rig.status().channels.ro_valve.blocked_by,
        Some(ReservoirBlocker::DutyRest)
    );

    rig.run(61);
    assert!(rig.relays.pin(RoValve));
    assert_eq!(rig.status().rest_remaining_s.ro, None);
}

#[test]
fn automation_waits_ten_dry_minutes_and_skips_during_a_water_change() {
    let auto = ReservoirSettings {
        auto_fill: true,
        ..RO_ONLY
    };
    let mut rig = Rig::new(auto);
    rig.run(598);
    assert!(rig.status().fill.is_none());
    rig.run(3);
    assert_eq!(
        rig.status().fill.map(|fill| fill.trigger),
        Some(FillTrigger::Auto)
    );

    let mut busy = Rig::new(auto);
    busy.water_change = true;
    busy.run(700);
    assert!(busy.status().fill.is_none());
    assert_eq!(
        busy.status().last_skipped.map(|skip| skip.reason),
        Some(SkipReason::WaterChange)
    );

    busy.water_change = false;
    busy.run(2);
    assert!(busy.status().fill.is_some());
}

#[test]
fn a_soft_failure_latches_automation_until_the_barrel_is_full() {
    let auto = ReservoirSettings {
        auto_fill: true,
        ..TARGETED
    };
    let mut rig = Rig::new(auto);
    rig.tds = Some(150.0);
    rig.run(602);
    assert!(rig.status().fill.is_some());

    rig.tds = None;
    rig.run(100);
    let status = rig.status();
    assert_eq!(
        status.last.map(|record| record.result),
        Some(FillResult::TdsUnavailable)
    );
    assert!(status.auto_latched);

    rig.run(700);
    assert!(rig.status().fill.is_none());
    assert_eq!(
        rig.status().last_skipped.map(|skip| skip.reason),
        Some(SkipReason::AutoLatched)
    );

    rig.levels = barrel(&FULL);
    rig.run(1);
    assert!(!rig.status().auto_latched);
}

#[test]
fn runtime_counts_only_while_a_channel_runs() {
    let mut rig = Rig::new(RO_ONLY);
    rig.start_fill().expect("started");
    rig.run(11);

    let status = rig.status();
    assert_eq!(status.channels.ro_valve.runtime_s, 10);
    assert_eq!(status.channels.tap_valve.runtime_s, 0);

    rig.service.reset_runtime(RoValve);
    assert_eq!(rig.status().channels.ro_valve.runtime_s, 0);
}
