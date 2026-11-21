use super::*;
use crate::domain::heater::{HeaterFault, HeaterMode, HeaterStatus, HeatersStatus};
use crate::domain::metrics::{LightingProfile, SystemMode, WaterLevelEntryDto};
use crate::domain::pump::{
    OperationEvent, OperationKind, OperationRecord, OperationResult, OperationState, PumpsTelemetry,
};

fn populated() -> TankTelemetryDto {
    let mut dto = TankTelemetryDto::empty();

    dto.timestamp_epoch_s = 1_750_000_000;
    dto.system_health.uptime_seconds = 3600;
    dto.system_health.wifi_rssi_dbm = -57;
    dto.system_health.free_heap_kb = 180;

    dto.temperature.tank_celsius = Some(26.5);
    dto.temperature.sump_celsius = Some(26.0);
    dto.heaters = HeatersStatus {
        tank: HeaterStatus {
            active: false,
            mode: HeaterMode::Auto,
            fault: None,
            override_expires_in_s: None,
            target_celsius: 27.0,
        },
        sump: HeaterStatus {
            active: true,
            mode: HeaterMode::Auto,
            fault: None,
            override_expires_in_s: None,
            target_celsius: 26.5,
        },
    };
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
            "aquarium_temperature tank_c=26.5,sump_c=26,healthy=true,target_tank_c=27,target_sump_c=26.5,heater_tank=false,heater_sump=true,heater_fault=0i 1750000000"
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
        !encoded.contains(" tank_c="),
        "a dead sensor must not read as 0 degrees: {encoded}"
    );
    assert!(encoded.contains("sump_c=26"), "got: {encoded}");
}

#[test]
fn a_non_finite_value_is_omitted() {
    let mut dto = populated();
    dto.heaters.tank.target_celsius = f32::NAN;

    let encoded = encode(&dto).expect("encodes");

    assert!(!encoded.contains("NaN"), "got: {encoded}");
    assert!(!encoded.contains("target_tank_c="), "got: {encoded}");
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
    dto.heaters.tank.active = true;
    dto.heaters.sump.fault = Some(HeaterFault::SensorFault);

    let encoded = encode(&dto).expect("encodes");

    assert!(encoded.contains("heater_tank=true"), "got: {encoded}");
    assert!(encoded.contains("heater_fault=1i"), "got: {encoded}");
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

#[test]
fn a_tds_reading_becomes_its_own_measurement() {
    let mut snapshot = TankTelemetryDto::empty();
    snapshot.timestamp_epoch_s = 1_750_000_000;
    snapshot.tds.ppm = Some(412.5);
    snapshot.tds.ec_us_cm = Some(825.0);

    let body = encode(&snapshot).expect("a timestamped snapshot encodes");

    assert!(
        body.contains("aquarium_tds ppm=412.5,ec=825 1750000000"),
        "got: {body}"
    );
}

#[test]
fn a_snapshot_without_a_tds_reading_emits_no_tds_measurement() {
    let mut snapshot = TankTelemetryDto::empty();
    snapshot.timestamp_epoch_s = 1_750_000_000;

    let body = encode(&snapshot).expect("a timestamped snapshot encodes");

    assert!(!body.contains("aquarium_tds"), "got: {body}");
}

#[test]
fn the_pump_state_is_its_own_measurement() {
    let mut dto = populated();
    dto.pumps = PumpsTelemetry {
        return_pump: true,
        uv: true,
        makeup: false,
        drain: true,
        operation_kind: Some(OperationKind::WaterChange),
        operation_state: OperationState::Draining,
    };

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains(
            "aquarium_pumps pump_return=true,uv=true,pump_makeup=false,pump_drain=true,water_change_phase=1i,top_off=false 1750000000"
        ),
        "got: {encoded}"
    );
}

#[test]
fn a_top_off_is_flagged_without_a_water_change_phase() {
    let mut dto = populated();
    dto.pumps.makeup = true;
    dto.pumps.operation_kind = Some(OperationKind::TopOff);
    dto.pumps.operation_state = OperationState::Refilling;

    let encoded = encode(&dto).expect("encodes");

    assert!(
        encoded.contains("water_change_phase=0i,top_off=true"),
        "got: {encoded}"
    );
}

#[test]
fn every_water_change_state_has_a_phase_number() {
    for (state, number) in [
        (OperationState::Idle, 0),
        (OperationState::Draining, 1),
        (OperationState::Settling, 2),
        (OperationState::Refilling, 3),
        (OperationState::Resuming, 4),
        (OperationState::Suspended, 5),
    ] {
        let mut dto = populated();
        dto.pumps.operation_kind = Some(OperationKind::WaterChange);
        dto.pumps.operation_state = state;

        let encoded = encode(&dto).expect("encodes");
        assert!(
            encoded.contains(&format!("water_change_phase={number}i")),
            "{state:?}: {encoded}"
        );
    }
}

fn finished_water_change() -> OperationEvent {
    OperationEvent {
        record: OperationRecord {
            kind: OperationKind::WaterChange,
            started_at: Some(1_750_000_000),
            drain_s: 300,
            settle_s: 30,
            refill_s: 280,
            result: OperationResult::Overflow,
            resumed: true,
        },
        finished_at: Some(1_750_000_610),
        drained_l: Some(50.0),
        refilled_l: None,
    }
}

#[test]
fn a_finished_operation_becomes_a_tagged_point() {
    assert_eq!(
        encode_operation(&finished_water_change()).as_deref(),
        Some(
            "aquarium_pump_operation,kind=water_change,result=overflow drain_s=300i,settle_s=30i,refill_s=280i,drained_l=50,resumed=true 1750000610"
        )
    );
}

#[test]
fn an_operation_without_a_finish_time_is_not_exported() {
    let mut event = finished_water_change();
    event.finished_at = None;

    assert_eq!(encode_operation(&event), None);
}
