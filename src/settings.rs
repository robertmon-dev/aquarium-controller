use std::time::Duration;

use aquarium_core::secret::Secret;
use esp_idf_svc::wifi::AuthMethod;

use crate::adapters::outbound::ds18b20::RomAddress;
use crate::adapters::outbound::xkc_y25::{FilterConfig, Polarity};

#[derive(Debug)]
pub struct Settings {
    pub wifi: WifiSettings,
    pub http: HttpSettings,
    pub sensors: SensorSettings,
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
                read_interval: Duration::from_secs(30),
                tank_rom: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00],
                sump_rom: [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01],
                target_celsius: 27.0,
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
        }
    }
}
