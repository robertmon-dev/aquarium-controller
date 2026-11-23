#![recursion_limit = "256"]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{
    InMemoryLightingStore, InMemoryTelemetryStore, InMemoryWaterLevelStore,
};
use aquarium_core::application::heater::probe_heaters;
use aquarium_core::application::{
    export_loop, heater_loop, pump_loop, reservoir_loop, CalibrationService, HeaterController,
    HeaterService, MetricsExporter, PumpController, PumpService, ReservoirController,
    ReservoirService, TelemetryCollector, TelemetryConfig, TelemetrySources, WaterLevelSampler,
};
use aquarium_core::domain::pump::{OperationEvent, PumpId, Supervision};
use aquarium_core::domain::reservoir::{FillEvent, ReservoirChannel};
use aquarium_core::ports::inbound::TelemetryQuery;
use aquarium_core::ports::outbound::{
    CalibrationSource, Clock, HeaterStatusSource, LightingOverrideSource, PhReader,
    ProbeVoltageSink, PumpOutputs, PumpStatusSource, PumpStore, ReservoirStatusSource,
    ReservoirStore, SystemHealthProvider, TdsReader, TdsSampleSink, TelemetrySink,
    TemperatureReader, TemperatureSampleSink, WaterLevelPins, WaterLevelSink, WaterLevelSource,
};
use aquarium_core::protocol::ads1115::Channel;
use aquarium_core::protocol::ds18b20::{
    convert_all, read_scratchpad, search_roms, temperature_from_scratchpad, RomAddress,
};
use esp_idf_hal::gpio::{InputPin, OutputPin};
use esp_idf_hal::i2c::{I2c, I2cConfig, I2cDriver};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver};
use esp_idf_hal::units::FromValueType;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::io::vfs::MountedEventfs;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::{self, link_patches, EspError};
use tokio::net::TcpListener;
use tokio::sync::{mpsc, Notify};
use tokio::task::LocalSet;
use tokio::time::{sleep, MissedTickBehavior};
use tracing::{error, info, warn};

use aquarium_controller::adapters::outbound::ads1115::{Ads1115Bus, AnalogFrontEnd};
use aquarium_controller::adapters::outbound::ds18b20::{Ds18b20Adapter, OneWireBus};
use aquarium_controller::adapters::outbound::flood::FloodInputs;
use aquarium_controller::adapters::outbound::influx::InfluxWriter;
use aquarium_controller::adapters::outbound::nvs_calibration::NvsCalibration;
use aquarium_controller::adapters::outbound::nvs_heater::NvsHeaterSettings;
use aquarium_controller::adapters::outbound::nvs_pump::NvsPumpStore;
use aquarium_controller::adapters::outbound::nvs_reservoir::NvsReservoirStore;
use aquarium_controller::adapters::outbound::pump_storage::PumpStorage;
use aquarium_controller::adapters::outbound::relay::{
    Contact, RelayOutputs, ReservoirRelayOutputs,
};
use aquarium_controller::adapters::outbound::reservoir_storage::ReservoirStorage;
use aquarium_controller::adapters::outbound::ssr::SsrOutputs;
use aquarium_controller::adapters::outbound::temperature::TemperatureSource;
use aquarium_controller::adapters::outbound::xkc_y25::Xkc25Adapter;
use aquarium_controller::infrastructure::clock::SystemClock;
use aquarium_controller::infrastructure::system_health::EspSystemHealth;
use aquarium_controller::infrastructure::wifi::WifiManager;
use aquarium_controller::settings::{RunMode, Settings};

const SETTINGS: Settings = Settings::from_env();

const HEALTH_INTERVAL: Duration = Duration::from_secs(30);
const NVS_ALARM_INTERVAL: Duration = Duration::from_secs(30);

const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(10);
const WATCHDOG_CHECK: Duration = Duration::from_secs(2);

const I2C_BAUDRATE_KHZ: u32 = 400;
const PROBE_INTERVAL: Duration = Duration::from_secs(2);

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

    if matches!(SETTINGS.run_mode, RunMode::AdcProbe) {
        let mut front_end = analog_front_end(
            peripherals.i2c0,
            peripherals.pins.gpio8,
            peripherals.pins.gpio9,
            Some(Channel::A1),
            Some(Channel::A0),
        )?;

        return probe_adc(&mut front_end).await;
    }

    if matches!(SETTINGS.run_mode, RunMode::SsrProbe) {
        let mut outputs = SsrOutputs::new(peripherals.pins.gpio17, peripherals.pins.gpio18)
            .context("Failed to initialize the heater SSR outputs")?;

        info!("SSR probe: one channel at a time, GPIO17 tank SSR, then GPIO18 sump SSR");
        probe_heaters(&mut outputs, PROBE_INTERVAL).await
    }

    if matches!(SETTINGS.run_mode, RunMode::RelayProbe) {
        let mut relays = RelayOutputs::new(
            peripherals.pins.gpio5,
            peripherals.pins.gpio6,
            peripherals.pins.gpio7,
            peripherals.pins.gpio16,
            SETTINGS.pumps.contacts,
        )
        .context("Failed to initialize the pump relay outputs")?;
        let mut barrel = ReservoirRelayOutputs::new(
            peripherals.pins.gpio40,
            peripherals.pins.gpio41,
            peripherals.pins.gpio42,
            peripherals.pins.gpio47,
            SETTINGS.reservoir.contacts,
        )
        .context("Failed to initialize the barrel relay outputs")?;

        return probe_relays(&mut relays, &mut barrel).await;
    }

    let flood_inputs = if components.flood.is_initialized() {
        Some(
            FloodInputs::new(peripherals.pins.gpio38, peripherals.pins.gpio48)
                .context("Failed to initialize flood alarm inputs")?,
        )
    } else {
        None
    };
    let mut relays = if components.pumps.is_initialized() || components.flood.is_running() {
        Some(
            RelayOutputs::new(
                peripherals.pins.gpio5,
                peripherals.pins.gpio6,
                peripherals.pins.gpio7,
                peripherals.pins.gpio16,
                SETTINGS.pumps.contacts,
            )
            .context("Failed to initialize the pump relay outputs")?,
        )
    } else {
        None
    };

    let initial_flood = flood_inputs
        .as_ref()
        .filter(|_| components.flood.is_running())
        .map(FloodInputs::read);
    if initial_flood.is_some() {
        if let Some(outputs) = &mut relays {
            for id in PumpId::ALL {
                if let Err(reason) = outputs.set(id, false) {
                    error!(
                        pump = id.name(),
                        reason, "Failed to hold pump output off during startup"
                    );
                }
            }
        }
    }
    let barrel_relays = if components.reservoir.is_initialized() {
        Some(
            ReservoirRelayOutputs::new(
                peripherals.pins.gpio40,
                peripherals.pins.gpio41,
                peripherals.pins.gpio42,
                peripherals.pins.gpio47,
                SETTINGS.reservoir.contacts,
            )
            .context("Failed to initialize the barrel relay outputs")?,
        )
    } else {
        None
    };

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

    let (nvs_partition, nvs_alarm) = match EspDefaultNvsPartition::take_with(false) {
        Ok(partition) => (Some(partition), None),
        Err(error) => (None, Some(nvs_unavailable_alarm(error))),
    };

    let wifi = if components.wifi.is_initialized() {
        let sys_loop = EspSystemEventLoop::take().context("System event loop already taken")?;

        Some(
            WifiManager::new(
                peripherals.modem,
                sys_loop,
                nvs_partition.clone(),
                &SETTINGS.wifi,
            )
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

    let clock = Arc::new(
        SystemClock::new(SETTINGS.timezone, components.wifi.is_initialized())
            .context("Failed to start SNTP")?,
    );

    let store = Arc::new(InMemoryTelemetryStore::new());
    let lighting = Arc::new(InMemoryLightingStore::new());
    let levels = Arc::new(InMemoryWaterLevelStore::new());

    let sampler = if components.water_level.is_initialized() {
        let pins = Xkc25Adapter::new([
            peripherals.pins.gpio11.degrade_input(),
            peripherals.pins.gpio12.degrade_input(),
            peripherals.pins.gpio13.degrade_input(),
            peripherals.pins.gpio14.degrade_input(),
            peripherals.pins.gpio15.degrade_input(),
            peripherals.pins.gpio21.degrade_input(),
            peripherals.pins.gpio10.degrade_input(),
            peripherals.pins.gpio39.degrade_input(),
        ])
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

    let calibration = Arc::new(CalibrationService::new(
        SETTINGS.sensors.calibration,
        NvsCalibration::new(nvs_partition.clone())
            .context("Failed to open the calibration namespace in NVS")?,
    ));

    info!(
        makeup_tank_celsius = calibration.calibration().makeup_tank_celsius,
        tds_factor = calibration.calibration().tds_factor,
        "Calibration loaded"
    );

    let heaters = Arc::new(HeaterService::new(
        SETTINGS.heaters.defaults,
        NvsHeaterSettings::new(nvs_partition.clone())
            .context("Failed to open the heater namespace in NVS")?,
    ));
    let heaters_republish = Arc::new(Notify::new());
    let heater_heartbeat = Arc::new(AtomicU32::new(0));

    let ssr = if components.heaters.is_initialized() {
        Some(
            SsrOutputs::new(peripherals.pins.gpio17, peripherals.pins.gpio18)
                .context("Failed to initialize the heater SSR outputs")?,
        )
    } else {
        None
    };

    let (heater_task, _idle_ssr) = match (ssr, components.heaters.is_running()) {
        (Some(outputs), true) => {
            info!("Heater control running");
            (
                Some(heater_loop(
                    HeaterController::new(outputs, Arc::clone(&heaters)),
                    store.subscribe_temperature(),
                    heaters.change_signal(),
                    Arc::clone(&heaters_republish),
                    SETTINGS.heaters.watchdog,
                    Arc::clone(&heater_heartbeat),
                )),
                None,
            )
        }
        (Some(outputs), false) => {
            info!("Heater outputs held low but not controlled");
            (None, Some(outputs))
        }
        (None, _) => (None, None),
    };

    let influx_exporting = SETTINGS.influx.is_some() && components.influx.is_running();
    let (pump_events, pump_events_rx) = if influx_exporting {
        let (sender, receiver) = mpsc::unbounded_channel::<OperationEvent>();
        (Some(sender), Some(receiver))
    } else {
        (None, None)
    };
    let pump_nvs = nvs_partition
        .clone()
        .and_then(|partition| match NvsPumpStore::new(partition) {
            Ok(store) => Some(store),
            Err(error) => {
                error!(%error, "Pump NVS namespace unavailable, pump automation disabled");
                None
            }
        });
    let pumps_automated = components.pumps.is_running() && pump_nvs.is_some();
    let pumps = Arc::new(PumpService::new(
        SETTINGS.pumps.defaults.clone(),
        pump_nvs.map_or(
            PumpStorage::Unavailable {
                flood_enabled: components.flood.is_running(),
            },
            PumpStorage::Nvs,
        ),
        Arc::clone(&clock),
        pump_events,
    ));
    if let Some(probes) = initial_flood {
        pumps.observe_flood(probes);
    }
    let pumps_republish = Arc::new(Notify::new());
    let pump_heartbeat = Arc::new(AtomicU32::new(0));

    let (fill_events, fill_events_rx) = if influx_exporting {
        let (sender, receiver) = mpsc::unbounded_channel::<FillEvent>();
        (Some(sender), Some(receiver))
    } else {
        (None, None)
    };
    let reservoir_nvs =
        nvs_partition.and_then(|partition| match NvsReservoirStore::new(partition) {
            Ok(store) => Some(store),
            Err(error) => {
                error!(%error, "Reservoir NVS namespace unavailable, barrel relays held released");
                None
            }
        });
    let reservoir_automated = components.reservoir.is_running()
        && reservoir_nvs.is_some()
        && components.water_level.is_running()
        && components.tds.is_running()
        && components.telemetry.is_running();
    let reservoir = Arc::new(ReservoirService::new(
        SETTINGS.reservoir.defaults,
        reservoir_nvs.map_or(ReservoirStorage::Unavailable, ReservoirStorage::Nvs),
        Arc::clone(&clock),
        fill_events,
        SETTINGS.reservoir.timing,
    ));
    let reservoir_republish = Arc::new(Notify::new());
    let reservoir_heartbeat = Arc::new(AtomicU32::new(0));

    let flood_heartbeat = Arc::new(AtomicU32::new(0));
    let mut supervised: Vec<Arc<dyn Supervised>> = Vec::new();
    let mut heartbeats = Vec::new();
    if components.flood.is_running() {
        heartbeats.push(Arc::clone(&flood_heartbeat));
    }
    if heater_task.is_some() {
        heartbeats.push(Arc::clone(&heater_heartbeat));
    }
    if pumps_automated && relays.is_some() {
        heartbeats.push(Arc::clone(&pump_heartbeat));
        supervised.push(pumps.clone());
    }
    if reservoir_automated {
        heartbeats.push(Arc::clone(&reservoir_heartbeat));
        supervised.push(reservoir.clone());
    }

    let watchdog_task = if heartbeats.is_empty() {
        None
    } else {
        supervise(&supervised, Supervision::Unavailable);
        let config = TWDTConfig {
            duration: WATCHDOG_TIMEOUT,
            panic_on_trigger: true,
            subscribed_idle_tasks: Default::default(),
        };

        match TWDTDriver::new(peripherals.twdt, &config) {
            Ok(driver) => Some(main_watchdog(driver, heartbeats, supervised)),
            Err(error) => {
                error!(%error, "Watchdog unavailable, pump and barrel supervision unavailable");
                None
            }
        }
    };

    let (pump_task, idle_relays) = match (relays, pumps_automated) {
        (Some(outputs), true) => {
            info!("Pump control running");
            (
                Some(pump_loop(
                    PumpController::new(outputs, Arc::clone(&pumps), SETTINGS.pumps.timing),
                    levels.subscribe(),
                    pumps.change_signal(),
                    Arc::clone(&pumps_republish),
                    SETTINGS.pumps.timing,
                    Arc::clone(&pump_heartbeat),
                )),
                None,
            )
        }
        (Some(outputs), false) => {
            info!("Pump automation disabled");
            (None, Some(outputs))
        }
        (None, _) => (None, None),
    };

    let (reservoir_task, _idle_barrel_relays) = match (barrel_relays, reservoir_automated) {
        (Some(outputs), true) => {
            info!("Barrel control running");
            (
                Some(reservoir_loop(
                    ReservoirController::new(
                        outputs,
                        Arc::clone(&reservoir),
                        SETTINGS.reservoir.timing,
                    ),
                    levels.subscribe(),
                    store.subscribe_tds(),
                    Arc::clone(&pumps),
                    pumps_automated.then(|| Arc::clone(&pumps)),
                    reservoir.change_signal(),
                    Arc::clone(&reservoir_republish),
                    SETTINGS.reservoir.timing,
                    Arc::clone(&reservoir_heartbeat),
                )),
                None,
            )
        }
        (Some(outputs), false) => {
            info!("Barrel relays held released but not controlled");
            (None, Some(outputs))
        }
        (None, _) => (None, None),
    };

    let flood_task = async {
        let mut idle = idle_relays
            .map(|outputs| PumpController::new(outputs, Arc::clone(&pumps), SETTINGS.pumps.timing));
        if !components.flood.is_running() && !pumps.flood_status().latched {
            return std::future::pending::<()>().await;
        }
        let mut previous = None;
        loop {
            flood_heartbeat.fetch_add(1, Ordering::Relaxed);
            if let Some(inputs) = flood_inputs
                .as_ref()
                .filter(|_| components.flood.is_running())
            {
                pumps.observe_flood(inputs.read());
            }
            let status = pumps.flood_status();
            let mut changed = previous != Some(status);
            if let Some(controller) = &mut idle {
                changed |= controller.tick_flood_only(
                    status.latched,
                    SETTINGS
                        .pumps
                        .contacts
                        .map(|contact| contact == Contact::NormallyClosed),
                );
            }
            if changed {
                pumps_republish.notify_one();
                reservoir.change_signal().notify_one();
                previous = Some(status);
            }
            sleep(Duration::from_millis(100)).await;
        }
    };
    let analog = if components.ph.is_initialized() || components.tds.is_initialized() {
        let front_end = analog_front_end(
            peripherals.i2c0,
            peripherals.pins.gpio8,
            peripherals.pins.gpio9,
            SETTINGS
                .sensors
                .analog
                .ph_channel
                .filter(|_| components.ph.is_running()),
            SETTINGS
                .sensors
                .analog
                .tds_channel
                .filter(|_| components.tds.is_running()),
        )
        .context("Failed to initialize the ADS1115 front end")?;

        info!(
            ph = components.ph.is_running(),
            tds = components.tds.is_running(),
            "ADS1115 front end ready"
        );

        Some(front_end)
    } else {
        None
    };

    let collector = if components.telemetry.is_initialized() {
        Some(TelemetryCollector::new(
            TelemetrySources {
                temperature,
                clock: Arc::clone(&clock),
                health: EspSystemHealth,
                overrides: Arc::clone(&lighting),
                levels: Arc::clone(&levels),
                analog,
                calibration: Arc::clone(&calibration),
                heaters: Arc::clone(&heaters),
                pumps: Arc::clone(&pumps),
                reservoir: Arc::clone(&reservoir),
            },
            Arc::clone(&store),
            TelemetryConfig {
                target_celsius: SETTINGS.sensors.target_celsius,
                ph_nominal_volts_per_ph: SETTINGS.sensors.ph.nominal_volts_per_ph,
                level_stale: SETTINGS.pumps.timing.level_stale,
            },
        ))
    } else {
        None
    };

    let telemetry_loop = match (collector, components.telemetry.is_running()) {
        (Some(collector), true) => Some(sensor_task(
            collector,
            lighting.change_signal(),
            Arc::clone(&heaters_republish),
            Arc::clone(&pumps_republish),
            reservoir_republish,
        )),
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
                Some(export_loop(
                    MetricsExporter::new(
                        writer,
                        influx.buffer_capacity,
                        influx.batch_max_snapshots,
                        influx.retry_backoff,
                    ),
                    store.subscribe(),
                    pump_events_rx.context("pump events are not wired for the Influx export")?,
                    fill_events_rx.context("fill events are not wired for the Influx export")?,
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
            Arc::clone(&calibration),
            Arc::clone(&heaters),
            Arc::clone(&pumps),
            Arc::clone(&reservoir),
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
        optional(watchdog_task),
        optional(telemetry_loop),
        optional(level_loop),
        optional(export_loop),
        optional(server),
        optional(heater_task),
        flood_task,
        optional(pump_task),
        optional(reservoir_task),
        optional(wifi_loop),
        optional(health_loop),
        optional(nvs_alarm),
    );

    Ok(())
}

async fn nvs_unavailable_alarm(error: EspError) {
    let mut ticker = tokio::time::interval(NVS_ALARM_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        error!(
            %error,
            "NVS partition unusable and left untouched: defaults in use, nothing is saved, \
             pump and barrel automation stopped; erase it deliberately as described in the README"
        );
    }
}

async fn optional<T, F: core::future::Future<Output = T>>(task: Option<F>) {
    match task {
        Some(task) => {
            task.await;
        }
        None => core::future::pending::<()>().await,
    }
}

trait Supervised {
    fn set_supervision(&self, supervision: Supervision);
}

impl<S: PumpStore, C: Clock> Supervised for PumpService<S, C> {
    fn set_supervision(&self, supervision: Supervision) {
        PumpService::set_supervision(self, supervision);
    }
}

impl<S: ReservoirStore, C: Clock> Supervised for ReservoirService<S, C> {
    fn set_supervision(&self, supervision: Supervision) {
        ReservoirService::set_supervision(self, supervision);
    }
}

fn supervise(services: &[Arc<dyn Supervised>], supervision: Supervision) {
    for service in services {
        service.set_supervision(supervision);
    }
}

async fn main_watchdog(
    mut driver: TWDTDriver<'static>,
    heartbeats: Vec<Arc<AtomicU32>>,
    supervised: Vec<Arc<dyn Supervised>>,
) {
    let mut subscription = match driver.watch_current_task() {
        Ok(subscription) => subscription,
        Err(error) => {
            error!(%error, "Watchdog could not watch the main task, pump and barrel supervision unavailable");
            return;
        }
    };

    supervise(&supervised, Supervision::Active);
    info!(
        timeout_s = WATCHDOG_TIMEOUT.as_secs(),
        loops = heartbeats.len(),
        "Watchdog armed"
    );

    let mut ticker =
        tokio::time::interval_at(tokio::time::Instant::now() + WATCHDOG_CHECK, WATCHDOG_CHECK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_beats: Vec<u32> = heartbeats
        .iter()
        .map(|beat| beat.load(Ordering::Relaxed))
        .collect();

    loop {
        ticker.tick().await;
        let beats: Vec<u32> = heartbeats
            .iter()
            .map(|beat| beat.load(Ordering::Relaxed))
            .collect();

        if beats
            .iter()
            .zip(&last_beats)
            .any(|(beat, last)| beat == last)
        {
            supervise(&supervised, Supervision::Unavailable);
            warn!("A control loop heartbeat stalled, watchdog not fed");
            continue;
        }

        last_beats = beats;

        match subscription.feed() {
            Ok(()) => supervise(&supervised, Supervision::Active),
            Err(error) => {
                error!(%error, "Watchdog feed failed");
                supervise(&supervised, Supervision::Unavailable);
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

async fn sensor_task<R, C, H, S, O, W, A, K, T, P, V>(
    mut collector: TelemetryCollector<R, C, H, S, O, W, A, K, T, P, V>,
    lighting_changed: Arc<Notify>,
    heaters_changed: Arc<Notify>,
    pumps_changed: Arc<Notify>,
    reservoir_changed: Arc<Notify>,
) where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink + ProbeVoltageSink + TemperatureSampleSink + TdsSampleSink,
    O: LightingOverrideSource,
    W: WaterLevelSource,
    A: PhReader + TdsReader,
    K: CalibrationSource,
    T: HeaterStatusSource,
    P: PumpStatusSource,
    V: ReservoirStatusSource,
{
    let mut ticker = tokio::time::interval(SETTINGS.sensors.read_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => collector.sample().await,
            _ = lighting_changed.notified() => collector.republish(),
            _ = heaters_changed.notified() => collector.republish(),
            _ = pumps_changed.notified() => collector.republish(),
            _ = reservoir_changed.notified() => collector.republish(),
        }
    }
}

fn analog_front_end<'d>(
    i2c: impl I2c + 'd,
    sda: impl InputPin + OutputPin + 'd,
    scl: impl InputPin + OutputPin + 'd,
    ph_channel: Option<Channel>,
    tds_channel: Option<Channel>,
) -> anyhow::Result<AnalogFrontEnd<'d>> {
    let analog = &SETTINGS.sensors.analog;
    let config = I2cConfig::new().baudrate(I2C_BAUDRATE_KHZ.kHz().into());

    let driver = I2cDriver::new(i2c, sda, scl, &config)
        .context("Failed to initialize the I2C bus for the ADS1115")?;

    Ok(AnalogFrontEnd::new(
        Ads1115Bus::new(driver, analog.address, analog.gain, analog.data_rate),
        ph_channel,
        tds_channel,
        analog.samples_per_reading,
    ))
}

async fn probe_adc(front_end: &mut AnalogFrontEnd<'_>) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    info!("ADC probe: A0 is the TDS probe, A1 is the pH probe");

    loop {
        ticker.tick().await;

        for (label, channel) in [("A0 tds", Channel::A0), ("A1 ph", Channel::A1)] {
            match front_end.probe(channel).await {
                Ok((raw, volts)) => info!(raw, volts, "{label}"),
                Err(reason) => error!(reason, "{label}"),
            }
        }
    }
}

const BARREL_PROBE: [(ReservoirChannel, bool); 8] = [
    (ReservoirChannel::RoValve, true),
    (ReservoirChannel::Booster, true),
    (ReservoirChannel::Booster, false),
    (ReservoirChannel::RoValve, false),
    (ReservoirChannel::TapValve, true),
    (ReservoirChannel::TapValve, false),
    (ReservoirChannel::Mixer, true),
    (ReservoirChannel::Mixer, false),
];

async fn probe_relays(
    relays: &mut RelayOutputs<'_>,
    barrel: &mut ReservoirRelayOutputs<'_>,
) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    info!(
        "Relay probe: module 1 IN1 GPIO5 return, IN2 GPIO6 uv, IN3 GPIO7 makeup, IN4 GPIO16 drain"
    );
    info!("Relay probe: module 2 IN1 GPIO40 ro_valve, IN2 GPIO41 booster, IN3 GPIO42 tap_valve, IN4 GPIO47 mixer");

    loop {
        for id in PumpId::ALL {
            for coil in [true, false] {
                ticker.tick().await;
                match relays.set_coil(id, coil) {
                    Ok(()) => info!(channel = id.name(), coil, "relay"),
                    Err(reason) => error!(channel = id.name(), reason, "relay"),
                }
            }
        }

        for (channel, coil) in BARREL_PROBE {
            ticker.tick().await;
            match barrel.set_coil(channel, coil) {
                Ok(()) => info!(channel = channel.name(), coil, "barrel relay"),
                Err(reason) => {
                    for output in [
                        ReservoirChannel::Booster,
                        ReservoirChannel::RoValve,
                        ReservoirChannel::TapValve,
                        ReservoirChannel::Mixer,
                    ] {
                        let _ = barrel.set_coil(output, false);
                    }
                    anyhow::bail!("barrel relay probe stopped at {}: {reason}", channel.name());
                }
            }
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
