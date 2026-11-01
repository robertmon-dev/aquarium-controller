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

mod water_level {
    use crate::domain::metrics::{TankTelemetryDto, WaterLevelEntryDto, WaterLevelMetricsDto};
    use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

    #[test]
    fn an_empty_snapshot_reports_no_water_and_no_health() {
        let dto = TankTelemetryDto::empty();

        for entry in [
            dto.water_level.sump_mechanical,
            dto.water_level.sump_biological,
            dto.water_level.sump_pump,
            dto.water_level.main_tank,
        ] {
            assert_eq!(
                entry,
                WaterLevelEntryDto {
                    wet: false,
                    faulty: false
                }
            );
        }
        assert!(
            !dto.water_level.sensors_healthy,
            "nothing has been sampled yet, so no sensor is known to be healthy"
        );
    }

    #[test]
    fn each_role_maps_to_its_own_field() {
        let mut snapshot = WaterLevelSnapshot::dry();
        snapshot.sensors[WaterLevelRole::SumpPump.index()].state = WaterLevelState::Wet;
        snapshot.sensors[WaterLevelRole::MainTank.index()].state = WaterLevelState::Wet;

        let dto = WaterLevelMetricsDto::from(&snapshot);

        assert!(dto.sump_pump.wet);
        assert!(dto.main_tank.wet);
        assert!(!dto.sump_mechanical.wet);
        assert!(!dto.sump_biological.wet);
    }

    #[test]
    fn a_faulty_sensor_is_reported_and_clears_the_health_flag() {
        let mut snapshot = WaterLevelSnapshot::dry();
        snapshot.sensors[WaterLevelRole::SumpBiological.index()].faulty = true;

        let dto = WaterLevelMetricsDto::from(&snapshot);

        assert!(dto.sump_biological.faulty);
        assert!(!dto.sump_mechanical.faulty);
        assert!(
            !dto.sensors_healthy,
            "one faulty sensor makes the whole group unhealthy"
        );
    }

    #[test]
    fn a_fully_working_group_is_healthy() {
        let dto = WaterLevelMetricsDto::from(&WaterLevelSnapshot::dry());

        assert!(dto.sensors_healthy);
    }
}

mod ph {
    use crate::domain::metrics::Ph;

    #[test]
    fn accepts_the_full_measurable_range() {
        for value in [0.0, 4.0, 7.0, 8.2, 14.0] {
            assert!(Ph::new(value).is_ok(), "{value} must be accepted");
        }
    }

    #[test]
    fn rejects_values_outside_the_scale() {
        assert!(Ph::new(-0.1).is_err());
        assert!(Ph::new(14.1).is_err());
    }

    #[test]
    fn rejects_nan() {
        assert!(Ph::new(f32::NAN).is_err());
    }

    #[test]
    fn rejects_infinity() {
        assert!(Ph::new(f32::INFINITY).is_err());
        assert!(Ph::new(f32::NEG_INFINITY).is_err());
    }

    #[test]
    fn keeps_the_value_it_was_given() {
        let ph = Ph::new(6.85).expect("valid");
        assert_eq!(ph.value(), 6.85);
    }
}
