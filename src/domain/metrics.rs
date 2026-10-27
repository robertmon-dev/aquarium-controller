use serde::{Deserialize, Serialize};

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct WaterTemperature(f32);

impl WaterTemperature {
    pub const MIN_SAFE_CELSIUS: f32 = 24.0;
    pub const MAX_SAFE_CELSIUS: f32 = 31.0;
    pub const POWER_ON_RESET_CELSIUS: f32 = 85.0;

    pub fn new(celsius: f32) -> Result<Self, &'static str> {
        if celsius.is_nan() {
            return Err("Temperature reading is NaN");
        }

        if (celsius - Self::POWER_ON_RESET_CELSIUS).abs() < f32::EPSILON {
            return Err("DS18B20 power-on reset value detected (85.0°C)");
        }

        if !(Self::MIN_SAFE_CELSIUS..=Self::MAX_SAFE_CELSIUS).contains(&celsius) {
            return Err(
                "Water temperature is outside the discus safe operating range (24.0°C - 31.0°C)",
            );
        }

        Ok(Self(celsius))
    }

    #[inline]
    pub fn as_celsius(&self) -> f32 {
        self.0
    }
}

impl fmt::Display for WaterTemperature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} °C", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightingProfile {
    Off,
    Sunrise,
    Daylight,
    Sunset,
    Moonlight,
}

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
    pub tank_celsius: Option<f32>,
    pub sump_celsius: Option<f32>,
    pub target_celsius: f32,
    pub heater_active: bool,
    pub sensors_healthy: bool,
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
    pub profile: LightingProfile,
    pub channel_white_pct: u8,
    pub channel_rgb_pct: u8,
}
