use crate::domain::reservoir::*;

const LIMITS: DutyLimits = DutyLimits {
    max_on_ms: 5_400_000,
    rest_ms: 900_000,
};

#[test]
fn ninety_minutes_of_work_force_a_fifteen_minute_rest() {
    let mut meter = DutyMeter::default();
    for _ in 0..5399 {
        meter.advance(true, 1000, LIMITS);
    }
    assert!(!meter.resting());

    meter.advance(true, 1000, LIMITS);
    assert!(meter.resting());
    assert_eq!(meter.rest_remaining_s(), Some(900));

    meter.advance(false, 899_000, LIMITS);
    assert!(meter.resting());
    meter.advance(false, 1000, LIMITS);
    assert!(!meter.resting());
    assert_eq!(meter.rest_remaining_s(), None);
}

#[test]
fn short_gaps_do_not_reset_the_work_counter() {
    let mut meter = DutyMeter::default();
    for _ in 0..45 {
        meter.advance(true, 120_000, LIMITS);
        meter.advance(false, 60_000, LIMITS);
    }
    assert!(meter.resting());
}

#[test]
fn a_rest_as_long_as_the_limit_resets_the_counter() {
    let mut meter = DutyMeter::default();
    meter.advance(true, 5_000_000, LIMITS);
    meter.advance(false, 900_000, LIMITS);
    meter.advance(true, 5_000_000, LIMITS);
    assert!(!meter.resting());
}

#[test]
fn a_rest_counts_down_only_while_the_valve_is_off() {
    let mut meter = DutyMeter::default();
    meter.advance(true, 5_400_000, LIMITS);
    assert!(meter.resting());

    meter.advance(true, 900_000, LIMITS);
    assert_eq!(meter.rest_remaining_s(), Some(900));

    meter.advance(false, 900_000, LIMITS);
    assert!(!meter.resting());
}

#[test]
fn after_a_rest_the_valve_works_a_full_stint_before_resting_again() {
    let mut meter = DutyMeter::default();
    meter.advance(true, 5_400_000, LIMITS);
    meter.advance(false, 900_000, LIMITS);
    assert!(!meter.resting());

    meter.advance(true, 5_399_000, LIMITS);
    assert!(!meter.resting());
    meter.advance(true, 1000, LIMITS);
    assert!(meter.resting());
}
