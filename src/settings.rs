use std::time::Duration;

use aquarium_core::secret::Secret;
use esp_idf_svc::wifi::AuthMethod;

use crate::adapters::outbound::ds18b20::RomAddress;
use crate::adapters::outbound::influx::InfluxSettings;
use crate::adapters::outbound::xkc_y25::{FilterConfig, Polarity};

#[derive(Debug)]
pub struct Settings {
    pub wifi: WifiSettings,
    pub http: HttpSettings,
    pub sensors: SensorSettings,
    /// `None` until all four `INFLUX_*` variables are present at build time.
    /// Export is an addition, not a requirement, so a missing endpoint must not
    /// break the firmware build - it only switches the exporter off.
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
            // 1000 snapshots at the 30 s telemetry tick is roughly 8 h of
            // tolerance for a network outage. Note these are snapshots, not
            // Influx points - one snapshot encodes to seven lines.
            buffer_capacity: 1000,
            batch_max_snapshots: 100,
            retry_backoff: Duration::from_secs(30),
            request_timeout: Duration::from_secs(10),
            // Sized for a TLS handshake plus the HTTP client's buffers. The
            // main task needs 128 KiB only because picoserve monomorphises a
            // huge future onto it; this thread does far less.
            worker_stack_size: 12 * 1024,
        }),
        _ => None,
    }
}
