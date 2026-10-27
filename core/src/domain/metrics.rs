use serde::{Deserialize, Serialize};

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct WaterTemperature(f32);

impl WaterTemperature {
    pub const MIN_SAFE_CELSIUS: f32 = 24.0;
    pub const MAX_SAFE_CELSIUS: f32 = 31.0;
    pub const POWER_ON_RESET_CELSIUS: f32 = 85.0;

    pub const MIN_PLAUSIBLE_CELSIUS: f32 = -10.0;
    pub const MAX_PLAUSIBLE_CELSIUS: f32 = 80.0;

    pub fn new(celsius: f32) -> Result<Self, &'static str> {
        if celsius.is_nan() {
            return Err("Temperature reading is NaN");
        }

        if (celsius - Self::POWER_ON_RESET_CELSIUS).abs() < f32::EPSILON {
            return Err("DS18B20 power-on reset value detected (85.0\u{b0}C)");
        }

        if !(Self::MIN_PLAUSIBLE_CELSIUS..=Self::MAX_PLAUSIBLE_CELSIUS).contains(&celsius) {
            return Err("Temperature reading is physically implausible");
        }

        Ok(Self(celsius))
    }

    #[inline]
    pub fn as_celsius(&self) -> f32 {
        self.0
    }

    #[inline]
    pub fn is_within_safe_range(&self) -> bool {
        (Self::MIN_SAFE_CELSIUS..=Self::MAX_SAFE_CELSIUS).contains(&self.0)
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

impl TankTelemetryDto {
    pub fn empty() -> Self {
        Self {
            timestamp_epoch_s: 0,
            system_mode: SystemMode::Normal,
            system_health: SystemHealthDto {
                uptime_seconds: 0,
                wifi_rssi_dbm: 0,
                free_heap_kb: 0,
            },
            temperature: TemperatureMetricsDto {
                tank_celsius: None,
                sump_celsius: None,
                target_celsius: 0.0,
                heater_active: false,
                sensors_healthy: false,
            },
            filtration: FiltrationMetricsDto {
                flow_rate_lph: None,
                flow_ok: false,
                filter_relay_active: false,
            },
            co2: Co2MetricsDto {
                solenoid_open: false,
                current_ph: None,
                drop_checker_ph_target: None,
            },
            ato: AtoMetricsDto {
                sump_water_low: false,
                reservoir_empty: false,
                pump_active: false,
                last_fill_duration_ms: 0,
            },
            lighting: LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
            },
            leak_detected: false,
        }
    }
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

#[cfg(test)]
mod water_temperature_tests {
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
        let t = WaterTemperature::new(21.0).expect("21 °C to realny odczyt, nie błąd");
        assert_eq!(t.as_celsius(), 21.0);
        assert!(!t.is_within_safe_range());
    }

    #[test]
    fn accepts_dangerously_hot_reading() {
        let t = WaterTemperature::new(34.0).expect("34 °C to realny odczyt, nie błąd");
        assert!(!t.is_within_safe_range());
    }

    #[test]
    fn safe_range_is_inclusive_at_both_ends() {
        assert!(WaterTemperature::new(24.0).unwrap().is_within_safe_range());
        assert!(WaterTemperature::new(31.0).unwrap().is_within_safe_range());
        assert!(WaterTemperature::new(27.5).unwrap().is_within_safe_range());
    }
}
