mod control;
mod monitoring;
mod network;
mod sensors;

use anyhow::Context;
use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{
    InMemoryLightingStore, InMemoryTelemetryStore, InMemoryWaterLevelStore,
};
use aquarium_core::application::pump::{startup_pump_outputs, stored_flood_latch};
use aquarium_core::application::{TelemetryCollector, TelemetryConfig, TelemetrySources};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::info;

use self::control::{Heaters, Pumps, Reservoir};
use self::monitoring::{health_watch, nvs_unavailable_alarm, Supervised};
use crate::board::Board;
use crate::SETTINGS;
use aquarium_controller::adapters::outbound::flood::FloodInputs;
use aquarium_controller::infrastructure::clock::SystemClock;
use aquarium_controller::infrastructure::system_health::EspSystemHealth;

pub(super) async fn run(board: Board) -> anyhow::Result<()> {
    let components = &SETTINGS.components;
    let flood_inputs = components
        .flood
        .is_initialized()
        .then(|| FloodInputs::new(board.flood.0, board.flood.1))
        .transpose()
        .context("Failed to initialize flood alarm inputs")?;
    let initial_flood = flood_inputs
        .as_ref()
        .filter(|_| components.flood.is_running())
        .map(FloodInputs::read);
    let (early_relays, pump_pins) = if initial_flood.is_some() {
        (
            startup_pump_outputs(false, true, || board.pumps.open())?,
            None,
        )
    } else {
        (None, Some(board.pumps))
    };
    let (nvs_partition, nvs_alarm) = match EspDefaultNvsPartition::take_with(false) {
        Ok(partition) => (Some(partition), None),
        Err(error) => (None, Some(nvs_unavailable_alarm(error))),
    };
    let pump_store = Pumps::open_store(nvs_partition.clone());
    let flood_latched = stored_flood_latch(&pump_store);
    let relays = match pump_pins {
        Some(pins) => {
            startup_pump_outputs(components.pumps.is_initialized(), flood_latched, || {
                pins.open()
            })?
        }
        None => early_relays,
    };

    let barrel_relays = components
        .reservoir
        .is_initialized()
        .then(|| board.reservoir.open())
        .transpose()?;
    let temperature = sensors::temperature(board.temperature)?;
    let wifi_loop = network::wifi(board.modem, nvs_partition.clone())?;
    let clock = Arc::new(
        SystemClock::new(SETTINGS.timezone, components.wifi.is_initialized())
            .context("Failed to start SNTP")?,
    );

    let (pump_events, pump_events_rx) = network::export_events();
    let pumps = Pumps::load(
        pump_store,
        flood_latched,
        Arc::clone(&clock),
        pump_events,
        initial_flood,
    );
    let store = Arc::new(InMemoryTelemetryStore::new());
    let lighting = Arc::new(InMemoryLightingStore::new());
    let levels = Arc::new(InMemoryWaterLevelStore::new());
    let level_loop = sensors::levels(board.levels, Arc::clone(&levels))?;
    let calibration = sensors::calibration(nvs_partition.clone())?;
    let heaters = Heaters::load(nvs_partition.clone())?;
    let (heater_task, _idle_ssr) = heaters.task(board.heaters, &store)?;

    let (fill_events, fill_events_rx) = network::export_events();
    let reservoir = Reservoir::load(nvs_partition, Arc::clone(&clock), fill_events);

    let flood_heartbeat = Arc::new(AtomicU32::new(0));
    let mut supervised: Vec<Arc<dyn Supervised>> = Vec::new();
    let mut heartbeats = Vec::new();
    if components.flood.is_running() {
        heartbeats.push(Arc::clone(&flood_heartbeat));
    }
    if heater_task.is_some() {
        heartbeats.push(Arc::clone(&heaters.heartbeat));
    }
    if pumps.automated && relays.is_some() {
        heartbeats.push(Arc::clone(&pumps.heartbeat));
        supervised.push(pumps.service.clone());
    }
    if reservoir.automated {
        heartbeats.push(Arc::clone(&reservoir.heartbeat));
        supervised.push(reservoir.service.clone());
    }
    let watchdog_task = monitoring::watchdog(board.twdt, heartbeats, supervised);

    let (pump_task, idle_relays) = pumps.task(relays, &levels);
    let (reservoir_task, _idle_barrel_relays) =
        reservoir.task(barrel_relays, &levels, &store, &pumps);
    let flood_task = pumps.watch_flood(
        flood_inputs,
        idle_relays,
        reservoir.service.change_signal(),
        flood_heartbeat,
    );
    let analog = sensors::analog(board.analog)?;
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
                heaters: Arc::clone(&heaters.service),
                pumps: Arc::clone(&pumps.service),
                reservoir: Arc::clone(&reservoir.service),
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
        (Some(collector), true) => Some(sensors::sensor_task(
            collector,
            lighting.change_signal(),
            Arc::clone(&heaters.republish),
            Arc::clone(&pumps.republish),
            Arc::clone(&reservoir.republish),
        )),
        (Some(_), false) => {
            info!("Telemetry collector built but not sampling");
            None
        }
        (None, _) => None,
    };
    let export_loop = network::export(&store, pump_events_rx, fill_events_rx)?;
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
            Arc::clone(&heaters.service),
            Arc::clone(&pumps.service),
            Arc::clone(&reservoir.service),
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

fn optional<'a, T: 'a, F: Future<Output = T> + 'a>(
    task: Option<F>,
) -> Pin<Box<dyn Future<Output = ()> + 'a>> {
    match task {
        Some(task) => Box::pin(async move {
            task.await;
        }),
        None => Box::pin(core::future::pending()),
    }
}
