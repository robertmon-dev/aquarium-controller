use super::*;

#[test]
fn active_high_reads_a_high_pin_as_wet() {
    assert_eq!(interpret(true, Polarity::ActiveHigh), WaterLevelState::Wet);
    assert_eq!(interpret(false, Polarity::ActiveHigh), WaterLevelState::Dry);
}

#[test]
fn active_low_reads_a_low_pin_as_wet() {
    assert_eq!(interpret(false, Polarity::ActiveLow), WaterLevelState::Wet);
    assert_eq!(interpret(true, Polarity::ActiveLow), WaterLevelState::Dry);
}

fn debounce_only(debounce_samples: u16) -> FilterConfig {
    FilterConfig {
        debounce_samples,
        flap_window_samples: 1_000,
        flap_max_transitions: u8::MAX,
    }
}

fn push_n(filter: &mut LevelFilter, state: WaterLevelState, times: u16) {
    for _ in 0..times {
        filter.push(state);
    }
}

#[test]
fn a_single_stray_sample_does_not_change_state() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, debounce_only(3));

    assert!(
        !filter.push(WaterLevelState::Wet),
        "one sample must not flip"
    );
    assert_eq!(filter.state(), WaterLevelState::Dry);
}

#[test]
fn the_configured_number_of_agreeing_samples_changes_state() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, debounce_only(3));

    push_n(&mut filter, WaterLevelState::Wet, 2);
    assert!(
        filter.push(WaterLevelState::Wet),
        "the third agreeing sample must report a change"
    );
    assert_eq!(filter.state(), WaterLevelState::Wet);
}

#[test]
fn one_sample_short_of_the_threshold_leaves_the_state_alone() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, debounce_only(3));

    push_n(&mut filter, WaterLevelState::Wet, 2);
    assert_eq!(filter.state(), WaterLevelState::Dry);
}

#[test]
fn a_broken_streak_resets_the_agreement_counter() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, debounce_only(3));

    push_n(&mut filter, WaterLevelState::Wet, 2);
    filter.push(WaterLevelState::Dry);
    push_n(&mut filter, WaterLevelState::Wet, 2);

    assert_eq!(
        filter.state(),
        WaterLevelState::Dry,
        "the streak restarted, so three more agreeing samples are required"
    );
}

#[test]
fn samples_agreeing_with_the_current_state_never_report_a_change() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, debounce_only(3));

    for _ in 0..10 {
        assert!(!filter.push(WaterLevelState::Dry));
    }
    assert_eq!(filter.state(), WaterLevelState::Dry);
}

fn flap_detection(flap_window_samples: u16, flap_max_transitions: u8) -> FilterConfig {
    FilterConfig {
        debounce_samples: 5,
        flap_window_samples,
        flap_max_transitions,
    }
}

fn push_alternating(filter: &mut LevelFilter, samples: u16) {
    for i in 0..samples {
        let raw = if i % 2 == 0 {
            WaterLevelState::Wet
        } else {
            WaterLevelState::Dry
        };
        filter.push(raw);
    }
}

#[test]
fn raw_chatter_above_the_threshold_marks_the_sensor_faulty() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, flap_detection(10, 3));

    push_alternating(&mut filter, 10);

    assert!(
        filter.is_faulty(),
        "ten transitions in a ten-sample window exceed a threshold of three"
    );
}

#[test]
fn a_faulty_sensor_keeps_reporting_its_last_stable_state() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, flap_detection(10, 3));

    push_alternating(&mut filter, 10);

    assert!(filter.is_faulty());
    assert_eq!(
        filter.state(),
        WaterLevelState::Dry,
        "a fault must not rewrite the reported level"
    );
}

#[test]
fn the_fault_clears_after_a_quiet_window() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, flap_detection(10, 3));

    push_alternating(&mut filter, 10);
    assert!(filter.is_faulty());

    push_n(&mut filter, WaterLevelState::Dry, 10);

    assert!(
        !filter.is_faulty(),
        "a full window without transitions clears the fault"
    );
}

#[test]
fn a_steady_signal_is_never_marked_faulty() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, flap_detection(10, 3));

    push_n(&mut filter, WaterLevelState::Dry, 100);

    assert!(!filter.is_faulty());
}

#[test]
fn transitions_at_the_threshold_are_not_a_fault() {
    let mut filter = LevelFilter::new(WaterLevelState::Dry, flap_detection(10, 3));

    push_n(&mut filter, WaterLevelState::Wet, 3);
    push_n(&mut filter, WaterLevelState::Dry, 3);
    push_n(&mut filter, WaterLevelState::Wet, 4);

    assert!(
        !filter.is_faulty(),
        "the threshold is the highest acceptable count, not the first rejected one"
    );
}
