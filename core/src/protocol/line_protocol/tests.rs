use super::*;
use crate::domain::metrics::{LightingProfile, WaterLevelEntryDto};

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
            "aquarium_temperature tank_c=26.5,sump_c=26,target_c=27,healthy=true 1750000000"
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
fn a_measurement_with_no_fields_left_produces_no_line() {
    let mut dto = TankTelemetryDto::empty();
    dto.timestamp_epoch_s = 1_750_000_000;
    dto.temperature.tank_celsius = None;
    dto.temperature.sump_celsius = None;
    dto.temperature.target_celsius = f32::INFINITY;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        !encoded.contains("aquarium_temperature"),
        "a line with no fields is invalid line protocol and would reject the whole batch: {encoded}"
    );
    assert!(
        encoded.contains("aquarium_health"),
        "the other measurements must survive: {encoded}"
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
