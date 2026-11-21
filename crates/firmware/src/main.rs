#![recursion_limit = "256"]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{
    InMemoryLightingStore, InMemoryTelemetryStore, InMemoryWaterLevelStore,
};
use aquarium_core::application::{
    heater_loop, pump_loop, CalibrationService, HeaterController, HeaterService, MetricsExporter,
    PumpController, PumpService, TelemetryCollector, TelemetryConfig, TelemetrySources,
    WaterLevelSampler,
};
use aquarium_core::domain::heater::HeaterId;
use aquarium_core::domain::metrics::TankTelemetryDto;
use aquarium_core::domain::pump::{OperationEvent, PumpId};
use aquarium_core::ports::inbound::TelemetryQuery;
use aquarium_core::ports::outbound::{
    CalibrationSource, Clock, HeaterOutputs, HeaterStatusSource, LightingOverrideSource,
    MetricsWriter, PhReader, ProbeVoltageSink, PumpStatusSource, SystemHealthProvider, TdsReader,
    TelemetrySink, TemperatureReader, TemperatureSampleSink, WaterLevelPins, WaterLevelSink,
    WaterLevelSource,
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
use tokio::sync::{mpsc, watch, Notify};
use tokio::task::LocalSet;
use tokio::time::{sleep, MissedTickBehavior};
use tracing::{error, info, warn};

use aquarium_controller::adapters::outbound::ads1115::{Ads1115Bus, AnalogFrontEnd};
use aquarium_controller::adapters::outbound::ds18b20::{Ds18b20Adapter, OneWireBus};
use aquarium_controller::adapters::outbound::influx::InfluxWriter;
use aquarium_controller::adapters::outbound::nvs_calibration::NvsCalibration;
use aquarium_controller::adapters::outbound::nvs_heater::NvsHeaterSettings;
use aquarium_controller::adapters::outbound::nvs_pump::NvsPumpStore;
use aquarium_controller::adapters::outbound::pump_storage::PumpStorage;
use aquarium_controller::adapters::outbound::relay::RelayOutputs;
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

const PUMP_WATCHDOG_TIMEOUT: Duration = Duration::from_secs(10);
const PUMP_WATCHDOG_CHECK: Duration = Duration::from_secs(2);

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

        return probe_ssr(&mut outputs).await;
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

        return probe_relays(&mut relays).await;
    }

    let relays = if components.pumps.is_initialized() {
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
    let nvs_available = nvs_partition.is_some();

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
    let pumps = Arc::new(PumpService::new(
        SETTINGS.pumps.defaults.clone(),
        match nvs_partition {
            Some(partition) => PumpStorage::Nvs(
                NvsPumpStore::new(partition).context("Failed to open the pump namespace in NVS")?,
            ),
            None => PumpStorage::Unavailable,
        },
        Arc::clone(&clock),
        pump_events,
    ));
    let pumps_republish = Arc::new(Notify::new());
    let pump_heartbeat = Arc::new(AtomicU32::new(0));

    let pumps_automated = components.pumps.is_running() && nvs_available;

    let pump_watchdog_task = if pumps_automated {
        let config = TWDTConfig {
            duration: PUMP_WATCHDOG_TIMEOUT,
            panic_on_trigger: true,
            subscribed_idle_tasks: Default::default(),
        };

        match TWDTDriver::new(peripherals.twdt, &config) {
            Ok(driver) => Some(pump_watchdog(driver, Arc::clone(&pump_heartbeat))),
            Err(error) => {
                error!(%error, "Pump watchdog unavailable");
                None
            }
        }
    } else {
        None
    };

    let (pump_task, _idle_relays) = match (relays, pumps_automated) {
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
            info!("Pump relays held released but not controlled");
            (None, Some(outputs))
        }
        (None, _) => (None, None),
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
            },
            Arc::clone(&store),
            TelemetryConfig {
                target_celsius: SETTINGS.sensors.target_celsius,
                ph_nominal_volts_per_ph: SETTINGS.sensors.ph.nominal_volts_per_ph,
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
                Some(export_task(
                    MetricsExporter::new(
                        writer,
                        influx.buffer_capacity,
                        influx.batch_max_snapshots,
                    ),
                    store.subscribe(),
                    pump_events_rx.context("pump events are not wired for the Influx export")?,
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
            Arc::clone(&calibration),
            Arc::clone(&heaters),
            Arc::clone(&pumps),
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
        optional(heater_task),
        optional(pump_task),
        optional(pump_watchdog_task),
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
             pump automation stopped; erase it deliberately as described in the README"
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

async fn pump_watchdog(mut driver: TWDTDriver<'static>, heartbeat: Arc<AtomicU32>) {
    let mut subscription = match driver.watch_current_task() {
        Ok(subscription) => subscription,
        Err(error) => {
            error!(%error, "Pump watchdog could not watch the main task");
            return;
        }
    };

    info!(
        timeout_s = PUMP_WATCHDOG_TIMEOUT.as_secs(),
        "Pump watchdog armed"
    );

    let mut ticker = tokio::time::interval_at(
        tokio::time::Instant::now() + PUMP_WATCHDOG_CHECK,
        PUMP_WATCHDOG_CHECK,
    );
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_beat = heartbeat.load(Ordering::Relaxed);

    loop {
        ticker.tick().await;
        let beat = heartbeat.load(Ordering::Relaxed);

        if beat == last_beat {
            warn!("Pump loop heartbeat stalled, watchdog not fed");
            continue;
        }

        last_beat = beat;

        if let Err(error) = subscription.feed() {
            error!(%error, "Pump watchdog feed failed");
        }
    }
}

async fn export_task<W>(
    mut exporter: MetricsExporter<W>,
    mut rx: watch::Receiver<TankTelemetryDto>,
    mut events: mpsc::UnboundedReceiver<OperationEvent>,
    backoff: Duration,
) where
    W: MetricsWriter,
{
    loop {
        let retrying = exporter.pending() > 0;

        let should_flush = tokio::select! {
            changed = rx.changed() => {
                if changed.is_err() {
                    return;
                }

                let snapshot = rx.borrow_and_update().clone();
                exporter.enqueue(&snapshot)
            }
            Some(event) = events.recv() => exporter.enqueue_operation(&event),
            () = tokio::time::sleep(backoff), if retrying => true,
        };

        if !should_flush {
            continue;
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

async fn sensor_task<R, C, H, S, O, W, A, K, T, P>(
    mut collector: TelemetryCollector<R, C, H, S, O, W, A, K, T, P>,
    lighting_changed: Arc<Notify>,
    heaters_changed: Arc<Notify>,
    pumps_changed: Arc<Notify>,
) where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink + ProbeVoltageSink + TemperatureSampleSink,
    O: LightingOverrideSource,
    W: WaterLevelSource,
    A: PhReader + TdsReader,
    K: CalibrationSource,
    T: HeaterStatusSource,
    P: PumpStatusSource,
{
    let mut ticker = tokio::time::interval(SETTINGS.sensors.read_interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            _ = ticker.tick() => collector.sample().await,
            _ = lighting_changed.notified() => collector.republish(),
            _ = heaters_changed.notified() => collector.republish(),
            _ = pumps_changed.notified() => collector.republish(),
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

async fn probe_ssr(outputs: &mut SsrOutputs<'_>) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    info!("SSR probe: one channel at a time, GPIO17 tank SSR, then GPIO18 sump SSR");

    loop {
        for (id, gpio) in [(HeaterId::Tank, 17), (HeaterId::Sump, 18)] {
            for on in [true, false] {
                ticker.tick().await;

                match outputs.set(id, on) {
                    Ok(()) => info!(heater = id.name(), gpio, on, "SSR {}", id.name()),
                    Err(reason) => error!(heater = id.name(), gpio, reason, "SSR {}", id.name()),
                }
            }
        }
    }
}

async fn probe_relays(relays: &mut RelayOutputs<'_>) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    info!("Relay probe: IN1 GPIO5 return, IN2 GPIO6 uv, IN3 GPIO7 makeup, IN4 GPIO16 drain");

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
