use super::WaterTemperature;

#[test]
fn rejects_nan() {
    assert!(WaterTemperature::new(f32::NAN).is_err());
}

#[test]
fn rejects_power_on_reset_sentinel() {
    assert!(WaterTemperature::new(85.0).is_err());
}

#[test]
fn rejects_physically_impossible() {
    assert!(WaterTemperature::new(-40.0).is_err());
    assert!(WaterTemperature::new(150.0).is_err());
}

#[test]
fn accepts_dangerously_cold_reading() {
    let t = WaterTemperature::new(21.0).expect("21 °C is a real reading, not an error");
    assert_eq!(t.as_celsius(), 21.0);
    assert!(!t.is_within_safe_range());
}

#[test]
fn accepts_dangerously_hot_reading() {
    let t = WaterTemperature::new(34.0).expect("34 °C is a real reading, not an error");
    assert!(!t.is_within_safe_range());
}

#[test]
fn safe_range_is_inclusive_at_both_ends() {
    assert!(WaterTemperature::new(24.0).unwrap().is_within_safe_range());
    assert!(WaterTemperature::new(31.0).unwrap().is_within_safe_range());
    assert!(WaterTemperature::new(27.5).unwrap().is_within_safe_range());
}
