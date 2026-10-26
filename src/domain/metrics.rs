use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SystemMode {
    Normal,
    Maintenance,
    EmergencyLockout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TankTelemetryDto {
    pub timestamp_epoch_s: u64,
    pub system_mode: SystemMode,
    pub system_health: SystemHealthDto,

    pub temperature: TemperatureMetricsDto,
    pub filtration: FiltrationMetricsDto,
    pub co2: Co2MetricsDto,
    pub ato: AtoMetricsDto,
    pub lighting: LightingMetricsDto,
    pub leak_detected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TemperatureMetricsDto {
    pub current_celsius: f32,
    pub target_celsius: f32,
    pub heater_active: bool,
    pub sensor_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FiltrationMetricsDto {
    pub flow_rate_lph: Option<f32>,
    pub flow_ok: bool,
    pub filter_relay_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Co2MetricsDto {
    pub solenoid_open: bool,
    pub current_ph: Option<f32>,
    pub drop_checker_ph_target: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AtoMetricsDto {
    pub sump_water_low: bool,
    pub reservoir_empty: bool,
    pub pump_active: bool,
    pub last_fill_duration_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SystemHealthDto {
    pub uptime_seconds: u64,
    pub wifi_rssi_dbm: i8,
    pub free_heap_kb: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LightingMetricsDto {
    pub current_profile: String,
    pub channel_white_pct: u8,
    pub channel_rgb_pct: u8,
}
