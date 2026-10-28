use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{InMemoryLightingStore, InMemoryTelemetryStore};
use aquarium_core::application::TelemetryCollector;
use aquarium_core::ports::outbound::{
    Clock, LightingOverrideSource, SystemHealthProvider, TelemetrySink, TemperatureReader,
};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::io::vfs::MountedEventfs;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{self, link_patches};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use tokio::task::LocalSet;
use tokio::time::MissedTickBehavior;
use tracing::{error, info};

use aquarium_controller::adapters::outbound::ds18b20::{Ds18b20Adapter, OneWireBus};
use aquarium_controller::infrastructure::clock::SystemClock;
use aquarium_controller::infrastructure::system_health::EspSystemHealth;
use aquarium_controller::infrastructure::wifi::WifiManager;
use aquarium_controller::settings::Settings;

const SETTINGS: Settings = Settings::from_env();
const ROM_DISCOVERY_MODE: bool = false;

const HEALTH_INTERVAL: Duration = Duration::from_secs(30);

fn main() -> anyhow::Result<()> {
    link_patches();
    init_logging();

    info!("Booted system controller");

    let _eventfs = MountedEventfs::mount(5).context("Failed to mount eventfd VFS for tokio")?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    let local = LocalSet::new();
    runtime.block_on(local.run_until(run()))
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .compact()
        .init();
}

fn log_health() {
    unsafe {
        let stack_free = sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut());
        let internal_free = sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL);
        let internal_min_free = sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_INTERNAL);

        if stack_free < 16384 {
            error!(stack_free, "main task stack headroom is low");
        }

        info!(stack_free, internal_free, internal_min_free, "health");
    }
}

async fn health_watch() {
    let mut ticker = tokio::time::interval(HEALTH_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        log_health();
    }
}

async fn run() -> anyhow::Result<()> {
    let peripherals = Peripherals::take().context("Peripherals already taken")?;

    let mut bus =
        OneWireBus::new(peripherals.pins.gpio4).context("Failed to initialize 1-Wire bus")?;

    if ROM_DISCOVERY_MODE {
        return discover_rom(&mut bus);
    }

    let sensors = Ds18b20Adapter::new(bus, SETTINGS.sensors.tank_rom, SETTINGS.sensors.sump_rom)
        .context("Failed to initialize DS18B20 sensors")?;
    info!("Temperature sensors ready");

    let sys_loop = EspSystemEventLoop::take().context("System event loop already taken")?;
    let nvs = EspDefaultNvsPartition::take().context("NVS partition already taken")?;

    let wifi = WifiManager::new(peripherals.modem, sys_loop, nvs, &SETTINGS.wifi)
        .context("Failed to initialize WiFi")?;

    let clock = SystemClock::new().context("Failed to start SNTP")?;
    let store = Arc::new(InMemoryTelemetryStore::new());
    let lighting = Arc::new(InMemoryLightingStore::new());
    let lighting_changed = lighting.change_signal();

    let listener = TcpListener::bind(("0.0.0.0", SETTINGS.http.port))
        .await
        .context("Failed to bind HTTP listener")?;

    let collector = TelemetryCollector::new(
        sensors,
        clock,
        EspSystemHealth,
        Arc::clone(&store),
        Arc::clone(&lighting),
        SETTINGS.sensors.target_celsius,
    );

    let server = http::run(
        listener,
        store,
        lighting,
        HttpConfig {
            token: SETTINGS.http.token.expose(),
            max_connections: SETTINGS.http.max_connections,
            sse_keepalive: SETTINGS.http.sse_keepalive,
        },
    );

    tokio::join!(sensor_task(collector, lighting_changed), wifi.run(), server);

    Ok(())
}

async fn sensor_task<R, C, H, S, O>(
    mut collector: TelemetryCollector<R, C, H, S, O>,
    lighting_changed: Arc<Notify>,
) where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink,
    O: LightingOverrideSource,
{
    let mut ticker = tokio::time::interval(SETTINGS.sensors.read_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => collector.sample().await,
            _ = lighting_changed.notified() => collector.republish(),
        }
    }
}

fn discover_rom(bus: &mut OneWireBus<'_>) -> anyhow::Result<()> {
    let rom = bus
        .read_single_rom()
        .context("ROM discovery failed; connect exactly ONE sensor")?;

    let formatted = rom
        .iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ");

    info!("Sensor ROM: [{formatted}]");
    Ok(())
}
