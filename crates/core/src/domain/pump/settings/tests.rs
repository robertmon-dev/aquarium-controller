use crate::domain::pump::*;
use crate::domain::water::WaterLevelRole;

fn defaults() -> PumpSettings {
    PumpSettings::DEFAULT
}

fn top_off(cron: &str, duration_s: u32) -> TopOffWindow {
    TopOffWindow {
        cron: cron.into(),
        duration_s,
    }
}

fn water_change(cron: &str) -> WaterChangeWindow {
    WaterChangeWindow {
        cron: cron.into(),
        drain_s: 300,
        settle_s: 30,
        refill_s: 420,
    }
}

#[test]
fn default_settings_are_valid() {
    assert_eq!(defaults().validate(), Ok(()));
    assert_eq!(defaults().resume_window_s, DEFAULT_RESUME_WINDOW_S);
}

#[test]
fn settings_validation_covers_every_field() {
    let mut too_many = defaults();
    too_many.top_off = (0..5).map(|_| top_off("0 8 * * *", 60)).collect();
    assert!(too_many.validate().is_err());

    let mut bad_cron = defaults();
    bad_cron.water_change = vec![water_change("0 25 * * *")];
    assert_eq!(bad_cron.validate(), Err("cron value out of range"));

    let mut long_top_off = defaults();
    long_top_off.top_off = vec![top_off("0 8 * * *", MAX_TOP_OFF_S + 1)];
    assert!(long_top_off.validate().is_err());

    let mut long_drain = defaults();
    long_drain.water_change = vec![WaterChangeWindow {
        drain_s: MAX_DRAIN_S + 1,
        ..water_change("0 10 * * sun")
    }];
    assert!(long_drain.validate().is_err());

    let mut long_cron = defaults();
    long_cron.top_off = vec![top_off(&"0 ".repeat(33), 60)];
    assert_eq!(
        long_cron.validate(),
        Err("cron must be at most 64 characters")
    );

    let mut zero_flow = defaults();
    zero_flow.flow_lph.makeup = Some(0);
    assert!(zero_flow.validate().is_err());

    let mut long_window = defaults();
    long_window.resume_window_s = MAX_RESUME_WINDOW_S + 1;
    assert!(long_window.validate().is_err());

    let mut fine = defaults();
    fine.top_off = vec![top_off("0 8,20 * * *", 120)];
    fine.water_change = vec![water_change("0 10 * * sun")];
    fine.flow_lph = FlowRates {
        makeup: Some(300),
        drain: Some(600),
    };
    assert_eq!(fine.validate(), Ok(()));
}

#[test]
fn the_refill_stop_sensor_must_sit_in_the_sump() {
    assert_eq!(
        defaults().refill_stop_sensor,
        Some(WaterLevelRole::SumpReturnLevel)
    );

    for role in [
        WaterLevelRole::SumpReturnLevel,
        WaterLevelRole::SumpMechanical,
        WaterLevelRole::SumpBiological,
    ] {
        let mut settings = defaults();
        settings.refill_stop_sensor = Some(role);
        assert_eq!(settings.validate(), Ok(()), "{role:?}");
    }

    for role in [
        WaterLevelRole::SumpPump,
        WaterLevelRole::MainTank,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::ReservoirFull,
    ] {
        let mut settings = defaults();
        settings.refill_stop_sensor = Some(role);
        assert_eq!(
            settings.validate(),
            Err("refill_stop_sensor must be sump_return_level, sump_mechanical or sump_biological"),
            "{role:?}"
        );
    }
}

#[test]
fn the_water_change_gap_is_bounded() {
    assert_eq!(
        defaults().min_water_change_gap_h,
        DEFAULT_WATER_CHANGE_GAP_H
    );

    for hours in [0, MAX_WATER_CHANGE_GAP_H] {
        let mut settings = defaults();
        settings.min_water_change_gap_h = hours;
        assert_eq!(settings.validate(), Ok(()));
    }

    let mut too_long = defaults();
    too_long.min_water_change_gap_h = MAX_WATER_CHANGE_GAP_H + 1;
    assert_eq!(
        too_long.validate(),
        Err("min_water_change_gap_h must not exceed 168")
    );
}

#[test]
fn the_reservoir_volume_is_optional_and_bounded() {
    assert_eq!(defaults().reservoir_ready_l, None);

    for litres in [1, MAX_RESERVOIR_L] {
        let mut settings = defaults();
        settings.reservoir_ready_l = Some(litres);
        assert_eq!(settings.validate(), Ok(()), "{litres}");
    }

    for litres in [0, MAX_RESERVOIR_L + 1] {
        let mut settings = defaults();
        settings.reservoir_ready_l = Some(litres);
        assert_eq!(
            settings.validate(),
            Err("reservoir_ready_l must be between 1 and 10000"),
            "{litres}"
        );
    }
}

fn reservoir_settings(drain_lph: Option<u32>, reservoir_l: Option<u32>) -> PumpSettings {
    let mut settings = defaults();
    settings.flow_lph.drain = drain_lph;
    settings.reservoir_ready_l = reservoir_l;
    settings
}

#[test]
fn a_drain_must_fit_in_the_ready_reservoir_when_both_are_known() {
    let settings = reservoir_settings(Some(1500), Some(50));

    assert!(settings.drain_fits_reservoir(120));
    assert!(!settings.drain_fits_reservoir(121));
    assert!(reservoir_settings(None, Some(50)).drain_fits_reservoir(MAX_DRAIN_S));
    assert!(reservoir_settings(Some(1500), None).drain_fits_reservoir(MAX_DRAIN_S));
}

#[test]
fn a_water_change_window_must_fit_in_the_ready_reservoir() {
    let window = |drain_s| WaterChangeWindow {
        drain_s,
        ..water_change("0 10 * * sun")
    };

    let mut fits = reservoir_settings(Some(1500), Some(50));
    fits.water_change = vec![window(120)];
    assert_eq!(fits.validate(), Ok(()));

    let mut too_long = reservoir_settings(Some(1500), Some(50));
    too_long.water_change = vec![window(120), window(121)];
    assert_eq!(
        too_long.validate(),
        Err("a water-change window drains more than reservoir_ready_l holds")
    );

    for unchecked in [
        reservoir_settings(None, Some(50)),
        reservoir_settings(Some(1500), None),
    ] {
        let mut settings = unchecked;
        settings.water_change = vec![window(MAX_DRAIN_S)];
        assert_eq!(settings.validate(), Ok(()));
    }
}

#[test]
fn a_patch_sets_keeps_or_clears_the_reservoir_volume() {
    let mut base = defaults();
    base.reservoir_ready_l = Some(40);

    let set: PumpSettingsPatch =
        serde_json::from_str(r#"{"reservoir_ready_l":47}"#).expect("patch");
    assert_eq!(set.merge_onto(&base).reservoir_ready_l, Some(47));

    let kept: PumpSettingsPatch = serde_json::from_str("{}").expect("patch");
    assert_eq!(kept.merge_onto(&base).reservoir_ready_l, Some(40));

    let cleared: PumpSettingsPatch =
        serde_json::from_str(r#"{"reservoir_ready_l":null}"#).expect("patch");
    assert_eq!(cleared.merge_onto(&base).reservoir_ready_l, None);
}

#[test]
fn a_patch_sets_keeps_or_clears_the_refill_stop_sensor() {
    let mut base = defaults();
    base.refill_stop_sensor = Some(WaterLevelRole::SumpMechanical);

    let set: PumpSettingsPatch =
        serde_json::from_str(r#"{"refill_stop_sensor":"sump_biological"}"#).expect("patch");
    assert_eq!(
        set.merge_onto(&base).refill_stop_sensor,
        Some(WaterLevelRole::SumpBiological)
    );

    let kept: PumpSettingsPatch = serde_json::from_str("{}").expect("patch");
    assert_eq!(
        kept.merge_onto(&base).refill_stop_sensor,
        Some(WaterLevelRole::SumpMechanical)
    );

    let cleared: PumpSettingsPatch =
        serde_json::from_str(r#"{"refill_stop_sensor":null}"#).expect("patch");
    assert_eq!(cleared.merge_onto(&base).refill_stop_sensor, None);

    let gap: PumpSettingsPatch =
        serde_json::from_str(r#"{"min_water_change_gap_h":24}"#).expect("patch");
    assert_eq!(gap.merge_onto(&base).min_water_change_gap_h, 24);
}

#[test]
fn settings_stored_before_the_refill_safeguards_still_load() {
    let stored: PumpSettings = serde_json::from_str(
        r#"{"top_off":[],"water_change":[],"flow_lph":{"makeup":null,"drain":null},"resume_window_s":120}"#,
    )
    .expect("old settings");

    assert_eq!(stored.resume_window_s, 120);
    assert_eq!(
        stored.refill_stop_sensor,
        Some(WaterLevelRole::SumpReturnLevel)
    );
    assert_eq!(stored.min_water_change_gap_h, DEFAULT_WATER_CHANGE_GAP_H);
    assert_eq!(stored.reservoir_ready_l, None);
}

#[test]
fn a_stored_null_refill_stop_sensor_stays_cleared() {
    let stored: PumpSettings = serde_json::from_str(
        r#"{"top_off":[],"water_change":[],"flow_lph":{"makeup":null,"drain":null},"resume_window_s":120,"refill_stop_sensor":null}"#,
    )
    .expect("settings");

    assert_eq!(stored.refill_stop_sensor, None);

    let round_trip: PumpSettings =
        serde_json::from_str(&serde_json::to_string(&stored).expect("encodes")).expect("decodes");
    assert_eq!(round_trip.refill_stop_sensor, None);
}

#[test]
fn a_patch_replaces_only_the_fields_it_carries() {
    let base = defaults();
    let patch = PumpSettingsPatch {
        top_off: Some(vec![top_off("0 8 * * *", 60)]),
        resume_window_s: Some(120),
        ..PumpSettingsPatch::default()
    };

    let merged = patch.merge_onto(&base);

    assert_eq!(merged.top_off, vec![top_off("0 8 * * *", 60)]);
    assert_eq!(merged.water_change, base.water_change);
    assert_eq!(merged.flow_lph, base.flow_lph);
    assert_eq!(merged.resume_window_s, 120);
}

#[test]
fn a_water_change_plan_defaults_its_settling_pause() {
    let plan: WaterChangePlan =
        serde_json::from_str(r#"{"drain_s":10,"refill_s":20}"#).expect("plan");

    assert_eq!(plan.settle_s, DEFAULT_SETTLE_S);
    assert_eq!(plan.validate(), Ok(()));

    for invalid in [
        WaterChangePlan { drain_s: 0, ..plan },
        WaterChangePlan {
            refill_s: 0,
            ..plan
        },
        WaterChangePlan {
            settle_s: MAX_SETTLE_S + 1,
            ..plan
        },
        WaterChangePlan {
            refill_s: MAX_REFILL_S + 1,
            ..plan
        },
    ] {
        assert!(invalid.validate().is_err(), "{invalid:?}");
    }
}

#[test]
fn top_off_duration_is_bounded() {
    assert!(validate_top_off(0).is_err());
    assert!(validate_top_off(MAX_TOP_OFF_S + 1).is_err());
    assert_eq!(validate_top_off(120), Ok(()));
}
