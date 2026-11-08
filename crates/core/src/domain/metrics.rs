use serde::{Deserialize, Serialize};

use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

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

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Ph(f32);

impl Ph {
    pub const MIN: f32 = 0.0;
    pub const MAX: f32 = 14.0;

    pub fn new(value: f32) -> Result<Self, &'static str> {
        if !value.is_finite() {
            return Err("pH reading is not a finite number");
        }

        if !(Self::MIN..=Self::MAX).contains(&value) {
            return Err("pH reading is outside the 0-14 scale");
        }

        Ok(Self(value))
    }

    #[inline]
    pub fn value(&self) -> f32 {
        self.0
    }
}

impl fmt::Display for Ph {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.2} pH", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum LightingProfile {
    Off,
    Sunrise,
    Daylight,
    Sunset,
    Moonlight,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum SystemMode {
    Normal,
    Maintenance,
    EmergencyLockout,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TankTelemetryDto {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub timestamp_epoch_s: u64,
    pub system_mode: SystemMode,
    pub system_health: SystemHealthDto,

    pub temperature: TemperatureMetricsDto,
    pub filtration: FiltrationMetricsDto,
    pub co2: Co2MetricsDto,
    pub ato: AtoMetricsDto,
    pub lighting: LightingMetricsDto,
    pub water_level: WaterLevelMetricsDto,
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
                ph_electrode_health: None,
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
                override_active: false,
            },
            water_level: WaterLevelMetricsDto {
                sump_mechanical: WaterLevelEntryDto::UNKNOWN,
                sump_biological: WaterLevelEntryDto::UNKNOWN,
                sump_pump: WaterLevelEntryDto::UNKNOWN,
                main_tank: WaterLevelEntryDto::UNKNOWN,
                sump_reserve: WaterLevelEntryDto::UNKNOWN,
                sensors_healthy: false,
            },
            leak_detected: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TemperatureMetricsDto {
    pub tank_celsius: Option<f32>,
    pub sump_celsius: Option<f32>,
    pub target_celsius: f32,
    pub heater_active: bool,
    pub sensors_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FiltrationMetricsDto {
    pub flow_rate_lph: Option<f32>,
    pub flow_ok: bool,
    pub filter_relay_active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct Co2MetricsDto {
    pub solenoid_open: bool,
    pub current_ph: Option<f32>,
    pub drop_checker_ph_target: Option<f32>,
    pub ph_electrode_health: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct AtoMetricsDto {
    pub sump_water_low: bool,
    pub reservoir_empty: bool,
    pub pump_active: bool,
    pub last_fill_duration_ms: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SystemHealthDto {
    #[cfg_attr(feature = "ts-export", ts(type = "number"))]
    pub uptime_seconds: u64,
    pub wifi_rssi_dbm: i8,
    pub free_heap_kb: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WaterLevelEntryDto {
    pub wet: bool,
    pub faulty: bool,
}

impl WaterLevelEntryDto {
    pub const UNKNOWN: Self = Self {
        wet: false,
        faulty: false,
    };
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WaterLevelMetricsDto {
    pub sump_mechanical: WaterLevelEntryDto,
    pub sump_biological: WaterLevelEntryDto,
    pub sump_pump: WaterLevelEntryDto,
    pub main_tank: WaterLevelEntryDto,
    pub sump_reserve: WaterLevelEntryDto,
    pub sensors_healthy: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct LightingScheduleDto {
    pub month: u8,
    pub step_minutes: u16,
    pub white_pct: Vec<u8>,
    pub rgb_pct: Vec<u8>,
}

impl From<&WaterLevelSnapshot> for WaterLevelMetricsDto {
    fn from(snapshot: &WaterLevelSnapshot) -> Self {
        let entry = |role: WaterLevelRole| {
            let reading = snapshot.get(role);
            WaterLevelEntryDto {
                wet: reading.state == WaterLevelState::Wet,
                faulty: reading.faulty,
            }
        };

        Self {
            sump_mechanical: entry(WaterLevelRole::SumpMechanical),
            sump_biological: entry(WaterLevelRole::SumpBiological),
            sump_pump: entry(WaterLevelRole::SumpPump),
            main_tank: entry(WaterLevelRole::MainTank),
            sump_reserve: entry(WaterLevelRole::SumpReserve),
            sensors_healthy: snapshot.all_healthy(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct LightingMetricsDto {
    pub profile: LightingProfile,
    pub channel_white_pct: u8,
    pub channel_rgb_pct: u8,
    pub override_active: bool,
}

#[cfg(test)]
mod tests;
