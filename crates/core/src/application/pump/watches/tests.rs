use std::time::Duration;

use crate::application::pump::fixtures::*;
use crate::domain::pump::{OperationState, WaterChangePlan};
use crate::domain::water::WaterLevelRole;
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::PumpStatusSource;

#[test]
fn the_return_pump_restarts_only_after_thirty_wet_seconds() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve]);

    rig.tick(Some(&healthy));
    rig.run(1, Some(&low));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(30, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn the_return_lockout_does_not_span_a_gap_in_level_data() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve]);

    rig.tick(Some(&healthy));
    rig.run(1, Some(&low));
    rig.run(20, Some(&healthy));
    rig.run(5, None);

    rig.run(30, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn makeup_refills_a_drained_sump_during_the_return_lockout() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve]);

    rig.tick(Some(&healthy));
    rig.run(1, Some(&low));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));

    assert_eq!(rig.pins(), [false, false, true, false]);
}

fn lockout_cycle(rig: &mut Rig) {
    rig.run(1, Some(&low_sump()));
    rig.run(40, Some(&healthy()));
}

#[test]
fn three_sump_low_lockouts_within_ten_minutes_flag_starved_circulation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));

    lockout_cycle(&mut rig);
    lockout_cycle(&mut rig);
    assert!(!starved(&rig));

    rig.run(1, Some(&low_sump()));
    assert!(starved(&rig));
    assert!(rig.service.pump_telemetry().circulation_starved);
}

#[test]
fn starved_circulation_clears_after_ten_wet_minutes() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));
    lockout_cycle(&mut rig);
    lockout_cycle(&mut rig);
    rig.run(1, Some(&low_sump()));
    assert!(starved(&rig));

    rig.run(600, Some(&healthy()));
    assert!(starved(&rig));

    rig.run(1, Some(&healthy()));
    assert!(!starved(&rig));
}

#[test]
fn lockouts_spread_over_more_than_ten_minutes_are_not_starvation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));

    for _ in 0..4 {
        lockout_cycle(&mut rig);
        rig.run(260, Some(&healthy()));
    }

    assert!(!starved(&rig));
}

#[test]
fn a_sump_that_stays_low_for_ten_minutes_starves_the_circulation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));

    rig.run(600, Some(&low_sump()));
    assert!(!starved(&rig));

    rig.run(1, Some(&low_sump()));
    assert!(starved(&rig));
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn a_low_sump_during_an_operation_is_not_starvation() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 700,
            settle_s: 0,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));

    for _ in 0..3 {
        rig.run(1, Some(&low_sump()));
        rig.run(10, Some(&healthy));
    }
    rig.run(650, Some(&low_sump()));

    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
    assert!(!starved(&rig));
}

#[test]
fn missing_levels_neither_raise_nor_clear_starvation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));
    rig.run(300, Some(&low_sump()));
    rig.run(400, None);
    rig.run(300, Some(&low_sump()));
    assert!(!starved(&rig));

    rig.run(301, Some(&low_sump()));
    assert!(starved(&rig));

    rig.run(300, Some(&healthy()));
    rig.run(700, None);
    rig.run(300, Some(&healthy()));
    assert!(starved(&rig));
}

#[test]
fn raising_starvation_republishes_the_status() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));
    rig.run(600, Some(&low_sump()));

    rig.now += Duration::from_secs(1);
    rig.clock.advance(1);
    assert!(rig.tick(Some(&low_sump())));
    assert!(starved(&rig));
}
