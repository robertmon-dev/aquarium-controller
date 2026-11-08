use std::fmt::{self, Write};

use crate::domain::metrics::{LightingProfile, SystemMode, TankTelemetryDto, WaterLevelEntryDto};

struct Fields(String);

impl Fields {
    fn new() -> Self {
        Self(String::new())
    }

    fn separate(&mut self) {
        if !self.0.is_empty() {
            self.0.push(',');
        }
    }

    fn float(&mut self, key: &str, value: f32) {
        if !value.is_finite() {
            return;
        }

        self.separate();
        let _ = write!(self.0, "{key}={value}");
    }

    fn optional_float(&mut self, key: &str, value: Option<f32>) {
        if let Some(value) = value {
            self.float(key, value);
        }
    }

    fn integer<T: fmt::Display>(&mut self, key: &str, value: T) {
        self.separate();
        let _ = write!(self.0, "{key}={value}i");
    }

    fn boolean(&mut self, key: &str, value: bool) {
        self.separate();
        let _ = write!(self.0, "{key}={value}");
    }

    fn string(&mut self, key: &str, value: &str) {
        self.separate();
        let _ = write!(self.0, "{key}=\"{value}\"");
    }

    fn finish(self) -> Option<String> {
        (!self.0.is_empty()).then_some(self.0)
    }
}

fn line(measurement: &str, fields: Fields, timestamp_s: u64) -> Option<String> {
    Some(format!("{measurement} {} {timestamp_s}", fields.finish()?))
}

fn profile_name(profile: LightingProfile) -> &'static str {
    match profile {
        LightingProfile::Off => "off",
        LightingProfile::Sunrise => "sunrise",
        LightingProfile::Daylight => "daylight",
        LightingProfile::Sunset => "sunset",
        LightingProfile::Moonlight => "moonlight",
    }
}

fn mode_name(mode: &SystemMode) -> &'static str {
    match mode {
        SystemMode::Normal => "normal",
        SystemMode::Maintenance => "maintenance",
        SystemMode::EmergencyLockout => "emergency_lockout",
    }
}

fn temperature(snapshot: &TankTelemetryDto) -> Fields {
    let mut fields = Fields::new();
    fields.optional_float("tank_c", snapshot.temperature.tank_celsius);
    fields.optional_float("sump_c", snapshot.temperature.sump_celsius);
    fields.float("target_c", snapshot.temperature.target_celsius);
    fields.boolean("healthy", snapshot.temperature.sensors_healthy);
    fields.boolean("heater", snapshot.temperature.heater_active);
    fields
}

fn filtration(snapshot: &TankTelemetryDto) -> Fields {
    let mut fields = Fields::new();
    fields.optional_float("flow_lph", snapshot.filtration.flow_rate_lph);
    fields.boolean("flow_ok", snapshot.filtration.flow_ok);
    fields.boolean("relay", snapshot.filtration.filter_relay_active);
    fields
}

fn ato(snapshot: &TankTelemetryDto) -> Fields {
    let mut fields = Fields::new();
    fields.boolean("sump_low", snapshot.ato.sump_water_low);
    fields.boolean("reservoir_empty", snapshot.ato.reservoir_empty);
    fields.boolean("pump_active", snapshot.ato.pump_active);
    fields.integer("last_fill_ms", snapshot.ato.last_fill_duration_ms);
    fields
}

fn health(snapshot: &TankTelemetryDto) -> Fields {
    let mut fields = Fields::new();
    fields.integer("uptime_s", snapshot.system_health.uptime_seconds);
    fields.integer("rssi_dbm", snapshot.system_health.wifi_rssi_dbm);
    fields.integer("free_heap_kb", snapshot.system_health.free_heap_kb);
    fields.string("mode", mode_name(&snapshot.system_mode));
    fields.boolean("leak", snapshot.leak_detected);
    fields.boolean("water_healthy", snapshot.water_level.sensors_healthy);
    fields
}

fn lighting(snapshot: &TankTelemetryDto) -> Fields {
    let mut fields = Fields::new();
    fields.string("profile", profile_name(snapshot.lighting.profile));
    fields.integer("white_pct", snapshot.lighting.channel_white_pct);
    fields.integer("rgb_pct", snapshot.lighting.channel_rgb_pct);
    fields.boolean("override", snapshot.lighting.override_active);
    fields
}

fn co2(snapshot: &TankTelemetryDto) -> Fields {
    let mut fields = Fields::new();
    fields.optional_float("ph", snapshot.co2.current_ph);
    fields.optional_float("electrode_health", snapshot.co2.ph_electrode_health);
    fields.optional_float("ph_target", snapshot.co2.drop_checker_ph_target);
    fields.boolean("solenoid_open", snapshot.co2.solenoid_open);
    fields
}

fn water_level(entry: WaterLevelEntryDto) -> Fields {
    let mut fields = Fields::new();
    fields.boolean("wet", entry.wet);
    fields.boolean("faulty", entry.faulty);
    fields
}

pub fn encode(snapshot: &TankTelemetryDto) -> Option<String> {
    let timestamp_s = snapshot.timestamp_epoch_s;
    if timestamp_s == 0 {
        return None;
    }

    let sensors = [
        ("sump_mechanical", snapshot.water_level.sump_mechanical),
        ("sump_biological", snapshot.water_level.sump_biological),
        ("sump_pump", snapshot.water_level.sump_pump),
        ("main_tank", snapshot.water_level.main_tank),
        ("sump_reserve", snapshot.water_level.sump_reserve),
    ];

    let mut lines = Vec::new();
    lines.extend(line(
        "aquarium_temperature",
        temperature(snapshot),
        timestamp_s,
    ));
    lines.extend(line("aquarium_health", health(snapshot), timestamp_s));
    lines.extend(line("aquarium_lighting", lighting(snapshot), timestamp_s));
    lines.extend(line("aquarium_co2", co2(snapshot), timestamp_s));
    lines.extend(line(
        "aquarium_filtration",
        filtration(snapshot),
        timestamp_s,
    ));
    lines.extend(line("aquarium_ato", ato(snapshot), timestamp_s));

    for (tag, entry) in sensors {
        lines.extend(line(
            &format!("aquarium_water_level,sensor={tag}"),
            water_level(entry),
            timestamp_s,
        ));
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

#[cfg(test)]
mod tests;
