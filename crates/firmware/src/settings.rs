use std::time::Duration;

use aquarium_core::secret::Secret;
use esp_idf_svc::wifi::AuthMethod;

use crate::adapters::outbound::ds18b20::RomAddress;
use crate::adapters::outbound::influx::InfluxSettings;
use crate::adapters::outbound::ph::PhCalibration;
use crate::adapters::outbound::xkc_y25::{FilterConfig, Polarity};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentMode {
    Off,
    Initialized,
    Running,
}

impl ComponentMode {
    pub const fn is_initialized(self) -> bool {
        !matches!(self, Self::Off)
    }

    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

#[derive(Debug)]
pub struct ComponentSettings {
    pub wifi: ComponentMode,
    pub http: ComponentMode,
    pub temperature: ComponentMode,
    pub telemetry: ComponentMode,
    pub water_level: ComponentMode,
    pub influx: ComponentMode,
    pub ph: ComponentMode,
    pub health: ComponentMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunMode {
    Normal,
    RomDiscovery,
}

#[derive(Debug)]
pub struct Settings {
    pub run_mode: RunMode,
    pub components: ComponentSettings,
    pub wifi: WifiSettings,
    pub http: HttpSettings,
    pub sensors: SensorSettings,
    pub influx: Option<InfluxSettings>,
}

#[derive(Debug, Clone)]
pub struct WifiSettings {
    pub ssid: &'static str,
    pub password: Secret<&'static str>,
    pub auth_method: AuthMethod,
}

#[derive(Debug)]
pub struct HttpSettings {
    pub port: u16,
    pub max_connections: usize,
    pub sse_keepalive: Duration,
    pub token: Secret<&'static str>,
}

#[derive(Debug)]
pub struct SensorSettings {
    pub read_interval: Duration,
    pub tank_rom: RomAddress,
    pub sump_rom: RomAddress,
    pub target_celsius: f32,
    pub level: LevelSettings,
    pub ph: PhSettings,
}

#[derive(Debug)]
pub struct PhSettings {
    pub calibration: PhCalibration,
    pub nominal_volts_per_ph: f32,
    pub samples_per_reading: u16,
}

#[derive(Debug)]
pub struct LevelSettings {
    pub poll_interval: Duration,
    pub polarity: Polarity,
    pub filter: FilterConfig,
}

impl Settings {
    pub const fn from_env() -> Self {
        Self {
            run_mode: RunMode::Normal,
            components: ComponentSettings {
                wifi: ComponentMode::Running,
                http: ComponentMode::Running,
                temperature: ComponentMode::Off,
                telemetry: ComponentMode::Running,
                water_level: ComponentMode::Off,
                influx: ComponentMode::Off,
                ph: ComponentMode::Off,
                health: ComponentMode::Off,
            },
            wifi: WifiSettings {
                ssid: env!("WIFI_SSID"),
                password: Secret::new(env!("WIFI_PASS")),
                auth_method: AuthMethod::WPA2Personal,
            },
            http: HttpSettings {
                port: 80,
                max_connections: 4,
                sse_keepalive: Duration::from_secs(15),
                token: Secret::new(env!("API_TOKEN")),
            },
            sensors: SensorSettings {
                read_interval: Duration::from_secs(2),
                tank_rom: [0x28, 0x50, 0x31, 0x83, 0x7C, 0x25, 0x0B, 0x62],
                sump_rom: [0x28, 0x9D, 0x32, 0x08, 0x7D, 0x25, 0x0B, 0x1F],
                target_celsius: 27.0,
                ph: PhSettings {
                    calibration: PhCalibration {
                        neutral_volts: 1.500,
                        acid_volts: 1.67748,
                        calibrated_at_celsius: 25.0,
                    },
                    nominal_volts_per_ph: 0.05916,
                    samples_per_reading: 64,
                },
                level: LevelSettings {
                    poll_interval: Duration::from_millis(100),
                    polarity: Polarity::ActiveLow,
                    filter: FilterConfig {
                        debounce_samples: 10,
                        flap_window_samples: 300,
                        flap_max_transitions: 60,
                    },
                },
            },
            influx: influx_from_env(),
        }
    }
}

const fn influx_from_env() -> Option<InfluxSettings> {
    match (
        option_env!("INFLUX_URL"),
        option_env!("INFLUX_ORG"),
        option_env!("INFLUX_BUCKET"),
        option_env!("INFLUX_TOKEN"),
    ) {
        (Some(url), Some(org), Some(bucket), Some(token)) => Some(InfluxSettings {
            url,
            org,
            bucket,
            token: Secret::new(token),
            buffer_capacity: 1000,
            batch_max_snapshots: 100,
            retry_backoff: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            worker_stack_size: 12 * 1024,
        }),
        _ => None,
    }
}
