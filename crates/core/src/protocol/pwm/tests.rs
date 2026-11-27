use super::duty_for_pct;

const MAX_12_BIT: u32 = 4095;

#[test]
fn zero_percent_is_zero_duty() {
    assert_eq!(duty_for_pct(0, MAX_12_BIT), 0);
}

#[test]
fn full_percent_is_full_duty() {
    assert_eq!(duty_for_pct(100, MAX_12_BIT), MAX_12_BIT);
}

#[test]
fn intermediate_percent_rounds_to_the_nearest_step() {
    assert_eq!(duty_for_pct(1, MAX_12_BIT), 41);
    assert_eq!(duty_for_pct(50, MAX_12_BIT), 2048);
}

#[test]
fn anything_above_one_hundred_is_clamped() {
    assert_eq!(duty_for_pct(255, MAX_12_BIT), MAX_12_BIT);
}
