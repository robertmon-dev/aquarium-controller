#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{
    InMemoryLightingStore, InMemoryTelemetryStore, VolatileCalibrationStore, VolatileHeaterStore,
    VolatilePumpStore,
};
use aquarium_core::application::{CalibrationService, HeaterService, PumpService};
use aquarium_core::domain::calibration::{Calibration, PhCalibration};
use aquarium_core::domain::cron::CivilTime;
use aquarium_core::domain::heater::{HeaterSettings, ZoneSettings};
use aquarium_core::domain::pump::PumpSettings;
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::Clock;

pub const TOKEN: &str = "test-token";
pub const ORIGIN: &str = "http://localhost:8081";
pub const OVERRIDE_PATH: &str = "/api/lighting/override";

pub fn config() -> HttpConfig {
    HttpConfig {
        token: TOKEN,
        max_connections: 4,
        sse_keepalive: Duration::from_secs(15),
        cors_origin: ORIGIN,
    }
}

pub type TestCalibration = CalibrationService<VolatileCalibrationStore>;

pub fn calibration_defaults() -> Calibration {
    Calibration {
        ph: PhCalibration {
            neutral_volts: 1.500,
            acid_volts: 1.67748,
            calibrated_at_celsius: 25.0,
        },
        makeup_tank_celsius: 25.0,
        tds_factor: 1.0,
    }
}

pub async fn start(store: Arc<InMemoryTelemetryStore>) -> SocketAddr {
    start_with_lighting(store, Arc::new(InMemoryLightingStore::new())).await
}

pub async fn start_with_lighting(
    store: Arc<InMemoryTelemetryStore>,
    lighting: Arc<InMemoryLightingStore>,
) -> SocketAddr {
    let calibration = Arc::new(CalibrationService::new(
        calibration_defaults(),
        VolatileCalibrationStore,
    ));

    start_with_calibration(store, lighting, calibration).await
}

pub type TestHeaters = HeaterService<VolatileHeaterStore>;

pub fn heater_defaults() -> HeaterSettings {
    HeaterSettings {
        tank: ZoneSettings {
            target_celsius: 27.0,
            hysteresis_celsius: 0.5,
        },
        sump: ZoneSettings {
            target_celsius: 26.0,
            hysteresis_celsius: 0.5,
        },
        cutoff_celsius: 30.0,
    }
}

pub fn heaters() -> Arc<TestHeaters> {
    Arc::new(HeaterService::new(heater_defaults(), VolatileHeaterStore))
}

pub const PUMP_EPOCH: u64 = 1_700_000_000;

pub struct FixedClock;

impl Clock for FixedClock {
    fn epoch_seconds(&self) -> Option<u64> {
        Some(PUMP_EPOCH)
    }

    fn uptime_seconds(&self) -> u64 {
        0
    }

    fn local_minute_of_day(&self) -> Option<u16> {
        None
    }

    fn local_month(&self) -> Option<u8> {
        None
    }

    fn local_civil_time(&self) -> Option<CivilTime> {
        None
    }
}

pub type TestPumps = PumpService<VolatilePumpStore, FixedClock>;

pub fn pumps() -> Arc<TestPumps> {
    Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        VolatilePumpStore::default(),
        FixedClock,
        None,
    ))
}

pub async fn start_with_pumps(pumps: Arc<TestPumps>) -> SocketAddr {
    start_with_all(
        Arc::new(InMemoryTelemetryStore::new()),
        Arc::new(InMemoryLightingStore::new()),
        Arc::new(CalibrationService::new(
            calibration_defaults(),
            VolatileCalibrationStore,
        )),
        heaters(),
        pumps,
    )
    .await
}

pub async fn start_with_calibration(
    store: Arc<InMemoryTelemetryStore>,
    lighting: Arc<InMemoryLightingStore>,
    calibration: Arc<TestCalibration>,
) -> SocketAddr {
    start_with_all(store, lighting, calibration, heaters(), pumps()).await
}

pub async fn start_with_heaters(heaters: Arc<TestHeaters>) -> SocketAddr {
    start_with_all(
        Arc::new(InMemoryTelemetryStore::new()),
        Arc::new(InMemoryLightingStore::new()),
        Arc::new(CalibrationService::new(
            calibration_defaults(),
            VolatileCalibrationStore,
        )),
        heaters,
        pumps(),
    )
    .await
}

pub async fn start_with_all(
    store: Arc<InMemoryTelemetryStore>,
    lighting: Arc<InMemoryLightingStore>,
    calibration: Arc<TestCalibration>,
    heaters: Arc<TestHeaters>,
    pumps: Arc<TestPumps>,
) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(
        listener,
        store,
        lighting,
        calibration,
        heaters,
        pumps,
        config(),
    ));

    addr
}

pub async fn start_with_pump_control<P: PumpControl + 'static>(pumps: Arc<P>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(
        listener,
        Arc::new(InMemoryTelemetryStore::new()),
        Arc::new(InMemoryLightingStore::new()),
        Arc::new(CalibrationService::new(
            calibration_defaults(),
            VolatileCalibrationStore,
        )),
        heaters(),
        pumps,
        config(),
    ));

    addr
}

pub fn body(method: &str, path: &str, auth: Option<&str>, json: &str) -> String {
    let auth_line = match auth {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };

    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{json}",
        json.len()
    )
}

pub fn get(path: &str, auth: Option<&str>) -> String {
    let auth_line = match auth {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };

    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}Connection: close\r\n\r\n")
}

pub async fn raw(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");

    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(500), stream.read_to_end(&mut buf)).await;

    String::from_utf8_lossy(&buf).into_owned()
}

pub fn preflight(path: &str, method: &str) -> String {
    format!(
        "OPTIONS {path} HTTP/1.1\r\nHost: localhost\r\nOrigin: {ORIGIN}\r\n\
         Access-Control-Request-Method: {method}\r\n\
         Access-Control-Request-Headers: authorization\r\n\
         Connection: close\r\n\r\n"
    )
}
