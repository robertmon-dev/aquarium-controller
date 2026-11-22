use crate::domain::reservoir::fixtures::half;
use crate::domain::reservoir::*;

fn with_full(full: bool, faulty: bool) -> Option<ReservoirConditions> {
    Some(ReservoirConditions {
        full: full || faulty,
        full_faulty: faulty,
        ..half()
    })
}

#[test]
fn the_trigger_waits_for_ten_dry_minutes() {
    let mut trigger = RefillDebounce::default();
    trigger.advance(with_full(false, false), 599_000);
    assert!(!trigger.due(600));
    trigger.advance(with_full(false, false), 1000);
    assert!(trigger.due(600));
}

#[test]
fn a_wet_faulty_or_missing_reading_restarts_the_trigger() {
    for interruption in [with_full(true, false), with_full(false, true), None] {
        let mut trigger = RefillDebounce::default();
        trigger.advance(with_full(false, false), 500_000);
        trigger.advance(interruption, 100);
        trigger.advance(with_full(false, false), 500_000);
        assert!(!trigger.due(600));
    }
}
