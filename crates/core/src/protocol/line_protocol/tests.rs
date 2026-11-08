use super::*;
use crate::domain::metrics::{LightingProfile, SystemMode, WaterLevelEntryDto};

fn populated() -> TankTelemetryDto {
    let mut dto = TankTelemetryDto::empty();

    dto.timestamp_epoch_s = 1_750_000_000;
    dto.system_health.uptime_seconds = 3600;
    dto.system_health.wifi_rssi_dbm = -57;
    dto.system_health.free_heap_kb = 180;

    dto.temperature.tank_celsius = Some(26.5);
    dto.temperature.sump_celsius = Some(26.0);
    dto.temperature.target_celsius = 27.0;
    dto.temperature.sensors_healthy = true;

    dto.lighting.profile = LightingProfile::Daylight;
    dto.lighting.channel_white_pct = 60;
    dto.lighting.channel_rgb_pct = 40;

    dto.water_level.sump_pump = WaterLevelEntryDto {
        wet: true,
        faulty: false,
    };
    dto.water_level.main_tank = WaterLevelEntryDto {
        wet: false,
        faulty: true,
    };

    dto
}

fn lines(dto: &TankTelemetryDto) -> Vec<String> {
    encode(dto)
        .expect("a dated snapshot must encode")
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn an_undated_snapshot_is_refused() {
    let mut dto = populated();
    dto.timestamp_epoch_s = 0;

    assert!(
        encode(&dto).is_none(),
        "an unsynced clock must not produce points dated 1970"
    );
}

#[test]
fn every_line_carries_the_timestamp() {
    for line in lines(&populated()) {
        assert!(
            line.ends_with(" 1750000000"),
            "missing timestamp on: {line}"
        );
    }
}

#[test]
fn temperature_fields_are_encoded_with_their_types() {
    let encoded = encode(&populated()).expect("encodes");

    assert!(
        encoded.contains(
            "aquarium_temperature tank_c=26.5,sump_c=26,target_c=27,healthy=true,heater=false 1750000000"
        ),
        "got: {encoded}"
    );
}

#[test]
fn integers_carry_the_i_suffix() {
    let encoded = encode(&populated()).expect("encodes");

    assert!(
        encoded.contains("aquarium_health uptime_s=3600i,rssi_dbm=-57i,free_heap_kb=180i"),
        "got: {encoded}"
    );
}

#[test]
fn the_lighting_profile_is_a_quoted_string() {
    let encoded = encode(&populated()).expect("encodes");

    assert!(
        encoded.contains(
            "aquarium_lighting profile=\"daylight\",white_pct=60i,rgb_pct=40i,override=false"
        ),
        "got: {encoded}"
    );
}

#[test]
fn each_water_level_sensor_gets_its_own_tagged_line() {
    let encoded = encode(&populated()).expect("encodes");

    for sensor in [
        "sump_mechanical",
        "sump_biological",
        "sump_pump",
        "main_tank",
        "sump_reserve",
    ] {
        assert!(
            encoded.contains(&format!("aquarium_water_level,sensor={sensor} ")),
            "no line for {sensor}: {encoded}"
        );
    }

    assert!(
        encoded.contains("aquarium_water_level,sensor=sump_pump wet=true,faulty=false"),
        "got: {encoded}"
    );
    assert!(
        encoded.contains("aquarium_water_level,sensor=main_tank wet=false,faulty=true"),
        "got: {encoded}"
    );
}

#[test]
fn a_missing_reading_omits_the_field_rather_than_writing_zero() {
    let mut dto = populated();
    dto.temperature.tank_celsius = None;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        !encoded.contains("tank_c="),
        "a dead sensor must not read as 0 degrees: {encoded}"
    );
    assert!(encoded.contains("sump_c=26"), "got: {encoded}");
}

#[test]
fn a_non_finite_value_is_omitted() {
    let mut dto = populated();
    dto.temperature.target_celsius = f32::NAN;

    let encoded = encode(&dto).expect("encodes");

    assert!(!encoded.contains("NaN"), "got: {encoded}");
    assert!(!encoded.contains("target_c="), "got: {encoded}");
}

#[test]
fn a_field_set_that_dropped_everything_produces_no_line() {
    let mut fields = Fields::new();
    fields.optional_float("tank_c", None);
    fields.float("target_c", f32::INFINITY);

    assert!(
        line("aquarium_temperature", fields, 1_750_000_000).is_none(),
        "a line with no fields is invalid line protocol and Influx would reject the whole batch"
    );
}

#[test]
fn a_field_set_keeps_the_line_once_anything_survives() {
    let mut fields = Fields::new();
    fields.optional_float("tank_c", None);
    fields.boolean("healthy", false);

    assert_eq!(
        line("aquarium_temperature", fields, 1_750_000_000).as_deref(),
        Some("aquarium_temperature healthy=false 1750000000")
    );
}

#[test]
fn no_line_is_left_empty_or_trailing() {
    let encoded = encode(&populated()).expect("encodes");

    assert!(!encoded.is_empty());
    assert!(!encoded.ends_with('\n'), "got: {encoded:?}");
    for line in encoded.lines() {
        assert!(!line.trim().is_empty(), "blank line in: {encoded:?}");
    }
}

#[test]
fn ph_and_electrode_health_are_exported() {
    let mut dto = populated();
    dto.co2.current_ph = Some(6.85);
    dto.co2.ph_electrode_health = Some(0.94);

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains("aquarium_co2 ph=6.85,electrode_health=0.94"),
        "got: {encoded}"
    );
}

#[test]
fn an_absent_ph_reading_omits_the_field() {
    let mut dto = populated();
    dto.co2.current_ph = None;
    dto.co2.ph_electrode_health = Some(0.94);

    let encoded = encode(&dto).expect("encodes");

    assert!(!encoded.contains("ph="), "got: {encoded}");
    assert!(encoded.contains("electrode_health=0.94"), "got: {encoded}");
}

#[test]
fn an_unconfigured_co2_section_still_reports_the_solenoid() {
    let mut dto = populated();
    dto.co2.current_ph = None;
    dto.co2.ph_electrode_health = None;
    dto.co2.drop_checker_ph_target = None;
    dto.co2.solenoid_open = true;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains("aquarium_co2 solenoid_open=true"),
        "the valve state is known even with no probe: {encoded}"
    );
    assert!(!encoded.contains("ph="), "got: {encoded}");
    assert!(!encoded.contains("electrode_health="), "got: {encoded}");
}

#[test]
fn filtration_is_exported() {
    let mut dto = populated();
    dto.filtration.flow_rate_lph = Some(420.5);
    dto.filtration.flow_ok = true;
    dto.filtration.filter_relay_active = true;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains("aquarium_filtration flow_lph=420.5,flow_ok=true,relay=true"),
        "got: {encoded}"
    );
}

#[test]
fn a_dead_flow_sensor_omits_the_rate_but_keeps_the_line() {
    let mut dto = populated();
    dto.filtration.flow_rate_lph = None;
    dto.filtration.flow_ok = false;

    let encoded = encode(&dto).expect("encodes");

    assert!(!encoded.contains("flow_lph="), "got: {encoded}");
    assert!(
        encoded.contains("aquarium_filtration flow_ok=false"),
        "the relay state is still worth recording: {encoded}"
    );
}

#[test]
fn ato_is_exported_with_the_fill_duration_as_an_integer() {
    let mut dto = populated();
    dto.ato.sump_water_low = true;
    dto.ato.reservoir_empty = false;
    dto.ato.pump_active = true;
    dto.ato.last_fill_duration_ms = 4200;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains(
            "aquarium_ato sump_low=true,reservoir_empty=false,pump_active=true,last_fill_ms=4200i"
        ),
        "got: {encoded}"
    );
}

#[test]
fn the_heater_state_is_exported() {
    let mut dto = populated();
    dto.temperature.heater_active = true;

    let encoded = encode(&dto).expect("encodes");

    assert!(encoded.contains("heater=true"), "got: {encoded}");
}

#[test]
fn the_ph_target_is_exported_when_configured() {
    let mut dto = populated();
    dto.co2.current_ph = Some(6.85);
    dto.co2.ph_electrode_health = Some(0.94);
    dto.co2.drop_checker_ph_target = Some(6.8);

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains(
            "aquarium_co2 ph=6.85,electrode_health=0.94,ph_target=6.8,solenoid_open=false"
        ),
        "got: {encoded}"
    );
}

#[test]
fn the_system_mode_is_a_quoted_string() {
    let mut dto = populated();
    dto.system_mode = SystemMode::EmergencyLockout;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains("mode=\"emergency_lockout\""),
        "got: {encoded}"
    );
}

#[test]
fn the_leak_flag_and_water_sensor_health_ride_on_the_health_line() {
    let mut dto = populated();
    dto.leak_detected = true;
    dto.water_level.sensors_healthy = false;

    let encoded = encode(&dto).expect("encodes");

    let health = encoded
        .lines()
        .find(|line| line.starts_with("aquarium_health "))
        .expect("a health line");

    assert!(health.contains("leak=true"), "got: {health}");
    assert!(health.contains("water_healthy=false"), "got: {health}");
}

#[test]
fn every_system_mode_has_a_name() {
    for (mode, expected) in [
        (SystemMode::Normal, "normal"),
        (SystemMode::Maintenance, "maintenance"),
        (SystemMode::EmergencyLockout, "emergency_lockout"),
    ] {
        assert_eq!(mode_name(&mode), expected);
    }
}
