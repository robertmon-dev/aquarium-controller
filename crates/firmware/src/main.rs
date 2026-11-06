use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{
    InMemoryLightingStore, InMemoryTelemetryStore, InMemoryWaterLevelStore,
};
use aquarium_core::application::{
    MetricsExporter, TelemetryCollector, TelemetryConfig, TelemetrySources, WaterLevelSampler,
};
use aquarium_core::domain::metrics::TankTelemetryDto;
use aquarium_core::ports::inbound::TelemetryQuery;
use aquarium_core::ports::outbound::{
    Clock, LightingOverrideSource, MetricsWriter, PhReader, SystemHealthProvider, TelemetrySink,
    TemperatureReader, WaterLevelPins, WaterLevelSink, WaterLevelSource,
};
use aquarium_core::protocol::ds18b20::{
    convert_all, read_scratchpad, search_roms, temperature_from_scratchpad, RomAddress,
};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::io::vfs::MountedEventfs;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{self, link_patches};
use tokio::net::TcpListener;
use tokio::sync::{watch, Notify};
use tokio::task::LocalSet;
use tokio::time::{sleep, MissedTickBehavior};
use tracing::{error, info};

use aquarium_controller::adapters::outbound::ds18b20::{Ds18b20Adapter, OneWireBus};
use aquarium_controller::adapters::outbound::influx::InfluxWriter;
use aquarium_controller::adapters::outbound::ph::PhProbe;
use aquarium_controller::adapters::outbound::temperature::TemperatureSource;
use aquarium_controller::adapters::outbound::xkc_y25::Xkc25Adapter;
use aquarium_controller::infrastructure::clock::SystemClock;
use aquarium_controller::infrastructure::system_health::EspSystemHealth;
use aquarium_controller::infrastructure::wifi::WifiManager;
use aquarium_controller::settings::{RunMode, Settings};

const SETTINGS: Settings = Settings::from_env();

const HEALTH_INTERVAL: Duration = Duration::from_secs(30);

const DISCOVERY_SENSOR_LIMIT: usize = 8;
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);
const DISCOVERY_CONVERSION_WAIT: Duration = Duration::from_millis(800);

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
    let components = &SETTINGS.components;

    if matches!(SETTINGS.run_mode, RunMode::RomDiscovery) {
        let mut bus =
            OneWireBus::new(peripherals.pins.gpio4).context("Failed to initialize 1-Wire bus")?;
        return discover_roms(&mut bus).await;
    }

    let temperature = if components.temperature.is_initialized() {
        let bus =
            OneWireBus::new(peripherals.pins.gpio4).context("Failed to initialize 1-Wire bus")?;
        let adapter =
            Ds18b20Adapter::new(bus, SETTINGS.sensors.tank_rom, SETTINGS.sensors.sump_rom)
                .context("Failed to initialize DS18B20 sensors")?;

        if components.temperature.is_running() {
            info!("Temperature sensors ready");
            TemperatureSource::Ds18b20(adapter)
        } else {
            info!("Temperature initialized, then released");
            TemperatureSource::Disabled
        }
    } else {
        TemperatureSource::Disabled
    };

    let wifi = if components.wifi.is_initialized() {
        let sys_loop = EspSystemEventLoop::take().context("System event loop already taken")?;
        let nvs = EspDefaultNvsPartition::take().context("NVS partition already taken")?;

        Some(
            WifiManager::new(peripherals.modem, sys_loop, nvs, &SETTINGS.wifi)
                .context("Failed to initialize WiFi")?,
        )
    } else {
        None
    };

    let wifi_loop = match (wifi, components.wifi.is_running()) {
        (Some(wifi), true) => Some(wifi.run()),
        (Some(_), false) => {
            info!("WiFi initialized but not connecting");
            None
        }
        (None, _) => None,
    };

    let store = Arc::new(InMemoryTelemetryStore::new());
    let lighting = Arc::new(InMemoryLightingStore::new());
    let levels = Arc::new(InMemoryWaterLevelStore::new());

    let sampler = if components.water_level.is_initialized() {
        let pins = Xkc25Adapter::new(
            peripherals.pins.gpio11,
            peripherals.pins.gpio12,
            peripherals.pins.gpio13,
            peripherals.pins.gpio14,
            peripherals.pins.gpio15,
        )
        .context("Failed to initialize XKC-Y25 level inputs")?;

        Some(WaterLevelSampler::new(
            pins,
            Arc::clone(&levels),
            SETTINGS.sensors.level.polarity,
            SETTINGS.sensors.level.filter,
        ))
    } else {
        None
    };

    let level_loop = match (sampler, components.water_level.is_running()) {
        (Some(sampler), true) => {
            info!("Water level sensors ready");
            Some(level_task(sampler))
        }
        (Some(_), false) => {
            info!("Water level inputs initialized but not sampling");
            None
        }
        (None, _) => None,
    };

    let ph = if components.ph.is_initialized() {
        let probe = PhProbe::new(
            peripherals.adc1,
            peripherals.pins.gpio1,
            SETTINGS.sensors.ph.samples_per_reading,
        )
        .context("Failed to initialize the pH probe")?;

        if components.ph.is_running() {
            info!("pH probe ready");
            Some(probe)
        } else {
            info!("pH probe initialized but not wired into telemetry");
            None
        }
    } else {
        None
    };

    let collector = if components.telemetry.is_initialized() {
        let clock = SystemClock::new().context("Failed to start SNTP")?;

        Some(TelemetryCollector::new(
            TelemetrySources {
                temperature,
                clock,
                health: EspSystemHealth,
                overrides: Arc::clone(&lighting),
                levels: Arc::clone(&levels),
                ph,
            },
            Arc::clone(&store),
            TelemetryConfig {
                target_celsius: SETTINGS.sensors.target_celsius,
                ph: SETTINGS.sensors.ph.calibration,
                ph_nominal_volts_per_ph: SETTINGS.sensors.ph.nominal_volts_per_ph,
            },
        ))
    } else {
        None
    };

    let telemetry_loop = match (collector, components.telemetry.is_running()) {
        (Some(collector), true) => Some(sensor_task(collector, lighting.change_signal())),
        (Some(_), false) => {
            info!("Telemetry collector built but not sampling");
            None
        }
        (None, _) => None,
    };

    let export_loop = match (&SETTINGS.influx, components.influx.is_initialized()) {
        (Some(influx), true) => {
            let writer =
                InfluxWriter::spawn(influx).context("Failed to start the Influx worker")?;

            if components.influx.is_running() {
                info!(bucket = influx.bucket, "InfluxDB export running");
                Some(export_task(
                    MetricsExporter::new(
                        writer,
                        influx.buffer_capacity,
                        influx.batch_max_snapshots,
                    ),
                    store.subscribe(),
                    influx.retry_backoff,
                ))
            } else {
                info!("InfluxDB worker started but not exporting");
                None
            }
        }
        (None, true) => {
            info!(
                "InfluxDB export unconfigured: set INFLUX_URL, INFLUX_ORG, INFLUX_BUCKET, INFLUX_TOKEN"
            );
            None
        }
        (_, false) => None,
    };

    let listener = if components.http.is_initialized() {
        Some(
            TcpListener::bind(("0.0.0.0", SETTINGS.http.port))
                .await
                .context("Failed to bind HTTP listener")?,
        )
    } else {
        None
    };

    let server = match (listener, components.http.is_running()) {
        (Some(listener), true) => Some(http::run(
            listener,
            Arc::clone(&store),
            Arc::clone(&lighting),
            HttpConfig {
                token: SETTINGS.http.token.expose(),
                max_connections: SETTINGS.http.max_connections,
                sse_keepalive: SETTINGS.http.sse_keepalive,
                cors_origin: SETTINGS.http.cors_origin,
            },
        )),
        (Some(_), false) => {
            info!("HTTP port bound but not serving");
            None
        }
        (None, _) => None,
    };

    let health_loop = components.health.is_running().then(health_watch);

    tokio::join!(
        optional(telemetry_loop),
        optional(level_loop),
        optional(export_loop),
        optional(server),
        optional(wifi_loop),
        optional(health_loop),
    );

    Ok(())
}

async fn optional<T, F: core::future::Future<Output = T>>(task: Option<F>) {
    match task {
        Some(task) => {
            task.await;
        }
        None => core::future::pending::<()>().await,
    }
}

async fn export_task<W>(
    mut exporter: MetricsExporter<W>,
    mut rx: watch::Receiver<TankTelemetryDto>,
    backoff: Duration,
) where
    W: MetricsWriter,
{
    loop {
        let snapshot = if exporter.pending() > 0 {
            match tokio::time::timeout(backoff, rx.changed()).await {
                Ok(Ok(())) => Some(rx.borrow_and_update().clone()),
                Ok(Err(_)) => return,
                Err(_) => None,
            }
        } else {
            match rx.changed().await {
                Ok(()) => Some(rx.borrow_and_update().clone()),
                Err(_) => return,
            }
        };

        if let Some(snapshot) = snapshot {
            if !exporter.enqueue(&snapshot) {
                continue;
            }
        }

        while exporter.pending() > 0 {
            match exporter.flush().await {
                Ok(0) => break,
                Ok(_) => continue,
                Err(_) => break,
            }
        }
    }
}

async fn level_task<P, S>(mut sampler: WaterLevelSampler<P, S>)
where
    P: WaterLevelPins,
    S: WaterLevelSink,
{
    let mut ticker = tokio::time::interval(SETTINGS.sensors.level.poll_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        sampler.tick();
    }
}

async fn sensor_task<R, C, H, S, O, W, P>(
    mut collector: TelemetryCollector<R, C, H, S, O, W, P>,
    lighting_changed: Arc<Notify>,
) where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink,
    O: LightingOverrideSource,
    W: WaterLevelSource,
    P: PhReader,
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

fn format_rom(rom: &RomAddress) -> String {
    rom.iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ")
}

async fn discover_roms(bus: &mut OneWireBus<'_>) -> anyhow::Result<()> {
    let mut found = [[0u8; 8]; DISCOVERY_SENSOR_LIMIT];
    let count = search_roms(bus, &mut found).context("SEARCH ROM failed")?;

    if count == 0 {
        anyhow::bail!("No DS18B20 responded on the 1-Wire bus");
    }

    for rom in &found[..count] {
        info!("Sensor ROM: [{}]", format_rom(rom));
    }

    info!(
        count,
        "Hold one probe in your hand; the ROM whose reading climbs is that sensor"
    );

    let mut ticker = tokio::time::interval(DISCOVERY_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        convert_all(bus).context("Failed to start a temperature conversion")?;
        sleep(DISCOVERY_CONVERSION_WAIT).await;

        for rom in &found[..count] {
            match read_scratchpad(bus, rom).and_then(|sp| temperature_from_scratchpad(&sp)) {
                Ok(temperature) => {
                    info!(celsius = temperature.as_celsius(), "[{}]", format_rom(rom))
                }
                Err(error) => error!(%error, "[{}]", format_rom(rom)),
            }
        }
    }
}
