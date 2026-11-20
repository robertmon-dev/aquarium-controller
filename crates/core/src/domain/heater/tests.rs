use super::*;

fn zone(target_celsius: f32, hysteresis_celsius: f32) -> ZoneSettings {
    ZoneSettings {
        target_celsius,
        hysteresis_celsius,
    }
}

fn settings() -> HeaterSettings {
    HeaterSettings {
        tank: zone(27.0, 0.5),
        sump: zone(26.0, 0.5),
        cutoff_celsius: 30.0,
    }
}

#[test]
fn heater_ids_parse_from_their_path_names_only() {
    assert_eq!("tank".parse::<HeaterId>(), Ok(HeaterId::Tank));
    assert_eq!("sump".parse::<HeaterId>(), Ok(HeaterId::Sump));
    assert!("boiler".parse::<HeaterId>().is_err());
    assert!("Tank".parse::<HeaterId>().is_err());
}

#[test]
fn heater_ids_index_their_own_slot() {
    for (slot, id) in HeaterId::ALL.into_iter().enumerate() {
        assert_eq!(id.index(), slot);
    }
}

#[test]
fn sensible_settings_validate() {
    assert_eq!(settings().validate(), Ok(()));
}

#[test]
fn a_target_outside_the_range_is_rejected() {
    for target in [14.9, 32.1, f32::NAN, f32::INFINITY] {
        let mut candidate = settings();
        candidate.sump.target_celsius = target;
        assert!(candidate.validate().is_err(), "accepted target {target}");
    }
}

#[test]
fn a_hysteresis_outside_the_range_is_rejected() {
    for hysteresis in [0.0, 0.09, 2.1, f32::NAN] {
        let mut candidate = settings();
        candidate.tank.hysteresis_celsius = hysteresis;
        assert!(
            candidate.validate().is_err(),
            "accepted hysteresis {hysteresis}"
        );
    }
}

#[test]
fn the_cutoff_must_sit_above_both_targets() {
    let mut candidate = settings();
    candidate.cutoff_celsius = 27.0;
    assert!(candidate.validate().is_err());

    candidate.cutoff_celsius = 27.1;
    assert_eq!(candidate.validate(), Ok(()));
}

#[test]
fn the_cutoff_is_capped() {
    let mut candidate = settings();
    candidate.cutoff_celsius = 35.1;
    assert!(candidate.validate().is_err());

    candidate.cutoff_celsius = f32::NAN;
    assert!(candidate.validate().is_err());
}

#[test]
fn a_patch_replaces_only_what_it_names() {
    let patch = HeaterSettingsPatch {
        tank: None,
        sump: Some(zone(25.0, 1.0)),
        cutoff_celsius: None,
    };

    let merged = patch.merge_onto(settings());

    assert_eq!(merged.tank, settings().tank);
    assert_eq!(merged.sump, zone(25.0, 1.0));
    assert_eq!(merged.cutoff_celsius, settings().cutoff_celsius);
}

#[test]
fn zone_follows_the_heater_id() {
    assert_eq!(settings().zone(HeaterId::Tank), settings().tank);
    assert_eq!(settings().zone(HeaterId::Sump), settings().sump);
}

#[test]
fn mode_reflects_the_override() {
    assert_eq!(HeaterMode::from_override(None), HeaterMode::Auto);
    assert_eq!(
        HeaterMode::from_override(Some(OverrideState::On)),
        HeaterMode::ManualOn
    );
    assert_eq!(
        HeaterMode::from_override(Some(OverrideState::Off)),
        HeaterMode::ManualOff
    );
}

#[test]
fn statuses_round_trip_through_json_in_snake_case() {
    let status = HeaterStatus {
        active: true,
        mode: HeaterMode::ManualOn,
        fault: Some(HeaterFault::OverTemperature),
        override_expires_in_s: Some(120),
        target_celsius: 27.0,
    };

    let json = serde_json::to_string(&status).expect("serializes");

    assert!(json.contains(r#""mode":"manual_on""#), "got: {json}");
    assert!(
        json.contains(r#""fault":"over_temperature""#),
        "got: {json}"
    );
    assert_eq!(
        serde_json::from_str::<HeaterStatus>(&json).expect("deserializes"),
        status
    );
}

const CUTOFF: f32 = 30.0;

fn tank() -> ZoneSettings {
    zone(27.0, 0.5)
}

fn auto(reading: f32, previously_on: bool) -> Decision {
    decide(Some(reading), tank(), CUTOFF, None, previously_on)
}

#[test]
fn a_cold_zone_turns_the_heater_on() {
    assert_eq!(
        auto(26.4, false),
        Decision {
            on: true,
            fault: None
        }
    );
}

#[test]
fn reaching_the_target_turns_the_heater_off() {
    assert!(!auto(27.0, true).on);
    assert!(!auto(28.0, true).on);
}

#[test]
fn inside_the_band_the_previous_state_holds() {
    assert!(auto(26.5, true).on);
    assert!(!auto(26.5, false).on);
    assert!(auto(26.9, true).on);
    assert!(!auto(26.9, false).on);
}

#[test]
fn a_missing_reading_is_a_sensor_fault() {
    for reading in [None, Some(f32::NAN), Some(f32::INFINITY)] {
        assert_eq!(
            decide(reading, tank(), CUTOFF, Some(OverrideState::On), true),
            Decision {
                on: false,
                fault: Some(HeaterFault::SensorFault)
            }
        );
    }
}

#[test]
fn the_cutoff_beats_a_manual_on() {
    assert_eq!(
        decide(Some(30.0), tank(), CUTOFF, Some(OverrideState::On), true),
        Decision {
            on: false,
            fault: Some(HeaterFault::OverTemperature)
        }
    );
}

#[test]
fn an_override_beats_the_thermostat() {
    assert!(decide(Some(29.0), tank(), CUTOFF, Some(OverrideState::On), false).on);
    assert!(!decide(Some(20.0), tank(), CUTOFF, Some(OverrideState::Off), true).on);
}

#[test]
fn an_on_override_needs_a_bounded_duration() {
    assert!(validate_override(OverrideState::On, None).is_err());
    assert!(validate_override(OverrideState::On, Some(0)).is_err());
    assert!(validate_override(OverrideState::On, Some(MAX_ON_OVERRIDE_S + 1)).is_err());
    assert_eq!(validate_override(OverrideState::On, Some(1)), Ok(()));
    assert_eq!(
        validate_override(OverrideState::On, Some(MAX_ON_OVERRIDE_S)),
        Ok(())
    );
}

#[test]
fn an_off_override_may_last_until_cleared() {
    assert_eq!(validate_override(OverrideState::Off, None), Ok(()));
    assert_eq!(validate_override(OverrideState::Off, Some(86_400)), Ok(()));
    assert!(validate_override(OverrideState::Off, Some(0)).is_err());
}

#[test]
fn accepted_edges_validate() {
    for (target, hysteresis) in [(15.0, 0.1), (32.0, 2.0)] {
        let mut candidate = settings();
        candidate.tank = zone(target, hysteresis);
        candidate.cutoff_celsius = 35.0;
        assert_eq!(candidate.validate(), Ok(()));
    }
}

#[test]
fn the_cutoff_must_sit_above_the_sump_target() {
    let mut candidate = settings();
    candidate.sump.target_celsius = 28.0;
    candidate.cutoff_celsius = 28.0;
    assert!(candidate.validate().is_err());
}

#[test]
fn a_nan_cutoff_fails_safe() {
    assert_eq!(
        decide(Some(20.0), tank(), f32::NAN, Some(OverrideState::On), true),
        Decision {
            on: false,
            fault: Some(HeaterFault::OverTemperature)
        }
    );
}
