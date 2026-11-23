use crate::domain::reservoir::*;

#[test]
fn default_settings_are_valid_and_keep_automation_off() {
    let settings = ReservoirSettings::DEFAULT;
    assert_eq!(settings.validate(), Ok(()));
    assert!(!settings.auto_fill);
    assert_eq!(settings.overtime_ms(), 8_640_000);
}

#[test]
fn settings_out_of_range_are_refused() {
    let base = ReservoirSettings::DEFAULT;
    let cases = [
        (
            ReservoirSettings {
                refill_debounce_s: 59,
                ..base
            },
            "refill_debounce_s must be between 60 and 7200",
        ),
        (
            ReservoirSettings {
                empty_fill_s: 599,
                ..base
            },
            "empty_fill_s must be between 600 and 43200",
        ),
        (
            ReservoirSettings {
                target_tds_ppm: Some(0),
                ..base
            },
            "target_tds_ppm must be between 1 and 2000",
        ),
        (
            ReservoirSettings {
                tolerance_ppm: 0,
                ..base
            },
            "tolerance_ppm must be between 1 and 200",
        ),
        (
            ReservoirSettings {
                tap_pulse_s: 61,
                ..base
            },
            "tap_pulse_s must be between 1 and 60",
        ),
        (
            ReservoirSettings {
                ro_pulse_s: 9,
                ..base
            },
            "ro_pulse_s must be between 10 and 1800",
        ),
        (
            ReservoirSettings { mix_s: 601, ..base },
            "mix_s must be between 10 and 600",
        ),
        (
            ReservoirSettings {
                inlet_max_on_s: 599,
                ..base
            },
            "inlet_max_on_s must be between 600 and 14400",
        ),
        (
            ReservoirSettings {
                inlet_rest_s: 59,
                ..base
            },
            "inlet_rest_s must be between 60 and 3600",
        ),
        (
            ReservoirSettings {
                flow_lph: InletFlows {
                    ro: Some(0),
                    tap: None,
                },
                ..base
            },
            "flow must be between 1 and 10000 L/h",
        ),
        (
            ReservoirSettings {
                max_duration_s: 7199,
                ..base
            },
            "max_duration_s must exceed the overtime budget plus forced inlet rests and mixing between pulses",
        ),
    ];

    for (settings, reason) in cases {
        assert_eq!(settings.validate(), Err(reason));
    }
}

#[test]
fn a_patch_changes_only_its_fields_and_null_clears_the_target() {
    let base = ReservoirSettings {
        target_tds_ppm: Some(150),
        ..ReservoirSettings::DEFAULT
    };

    let patch: ReservoirSettingsPatch =
        serde_json::from_str(r#"{"auto_fill":true,"mix_s":90}"#).expect("patch");
    let merged = patch.merge_onto(&base);
    assert!(merged.auto_fill);
    assert_eq!(merged.mix_s, 90);
    assert_eq!(merged.target_tds_ppm, Some(150));

    let cleared: ReservoirSettingsPatch =
        serde_json::from_str(r#"{"target_tds_ppm":null}"#).expect("patch");
    assert_eq!(cleared.merge_onto(&base).target_tds_ppm, None);
}

#[test]
fn stored_settings_missing_new_fields_load_with_defaults() {
    let stored: ReservoirSettings =
        serde_json::from_str(r#"{"auto_fill":true}"#).expect("settings");
    assert!(stored.auto_fill);
    assert_eq!(stored.inlet_rest_s, ReservoirSettings::DEFAULT.inlet_rest_s);
}

const TOO_SHORT: &str =
    "max_duration_s must exceed the overtime budget plus forced inlet rests and mixing between pulses";

#[test]
fn defaults_pass_with_and_without_a_target() {
    assert_eq!(ReservoirSettings::DEFAULT.validate(), Ok(()));
    let targeted = ReservoirSettings {
        target_tds_ppm: Some(150),
        ..ReservoirSettings::DEFAULT
    };
    assert_eq!(targeted.validate(), Ok(()));
}

#[test]
fn without_a_target_the_duration_must_cover_the_overtime_budget_and_the_forced_rests() {
    let with = |max_duration_s| ReservoirSettings {
        empty_fill_s: 7200,
        max_duration_s,
        ..ReservoirSettings::DEFAULT
    };
    assert_eq!(with(9540).validate(), Err(TOO_SHORT));
    assert_eq!(with(9541).validate(), Ok(()));
}

#[test]
fn with_a_target_the_duration_also_covers_mixing_and_measuring_between_pulses() {
    let with = |max_duration_s| ReservoirSettings {
        target_tds_ppm: Some(150),
        max_duration_s,
        ..ReservoirSettings::DEFAULT
    };
    assert_eq!(with(22_500).validate(), Err(TOO_SHORT));
    assert_eq!(with(22_501).validate(), Ok(()));
}

#[test]
fn a_long_rest_needs_a_longer_duration() {
    let long_rest = ReservoirSettings {
        inlet_max_on_s: 600,
        inlet_rest_s: 3600,
        empty_fill_s: 7200,
        max_duration_s: 43_200,
        ..ReservoirSettings::DEFAULT
    };
    assert_eq!(long_rest.validate(), Err(TOO_SHORT));
}

#[test]
fn the_fill_plan_carries_the_settings_in_milliseconds() {
    let plan = ReservoirSettings {
        target_tds_ppm: Some(150),
        ..ReservoirSettings::DEFAULT
    }
    .fill_plan();
    assert_eq!(plan.pulse_ms(Inlet::Tap), 5_000);
    assert_eq!(plan.pulse_ms(Inlet::Ro), 120_000);
}
