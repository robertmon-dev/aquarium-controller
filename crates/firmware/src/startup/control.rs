use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aquarium_controller::adapters::outbound::flood::FloodInputs;
use aquarium_controller::adapters::outbound::nvs_heater::NvsHeaterSettings;
use aquarium_controller::adapters::outbound::nvs_pump::NvsPumpStore;
use aquarium_controller::adapters::outbound::nvs_reservoir::NvsReservoirStore;
use aquarium_controller::adapters::outbound::pump_storage::PumpStorage;
use aquarium_controller::adapters::outbound::relay::{
    Contact, RelayOutputs, ReservoirRelayOutputs,
};
use aquarium_controller::adapters::outbound::reservoir_storage::ReservoirStorage;
use aquarium_controller::adapters::outbound::ssr::SsrOutputs;
use aquarium_controller::infrastructure::clock::SystemClock;
use aquarium_core::adapters::outbound::memory::{InMemoryTelemetryStore, InMemoryWaterLevelStore};
use aquarium_core::application::{
    heater_loop, pump_loop, reservoir_loop, HeaterController, HeaterService, PumpController,
    PumpService, ReservoirController, ReservoirService,
};
use aquarium_core::domain::flood::FloodProbes;
use aquarium_core::domain::pump::{OperationEvent, PumpId};
use aquarium_core::domain::reservoir::FillEvent;
use aquarium_core::ports::outbound::PumpOutputs;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use tokio::sync::{mpsc::UnboundedSender, Notify};
use tokio::time::sleep;
use tracing::{error, info};

use crate::board::{HeaterPins, PumpPins};
use crate::SETTINGS;

pub(super) struct Control<S> {
    pub service: Arc<S>,
    pub automated: bool,
    pub republish: Arc<Notify>,
    pub heartbeat: Arc<AtomicU32>,
}

impl<S> Control<S> {
    fn new(service: S, automated: bool) -> Self {
        Self {
            service: Arc::new(service),
            automated,
            republish: Arc::new(Notify::new()),
            heartbeat: Arc::new(AtomicU32::new(0)),
        }
    }
}

pub(super) fn pump_outputs(
    pins: PumpPins,
    flood_inputs: Option<&FloodInputs<'_>>,
) -> anyhow::Result<(Option<RelayOutputs<'static>>, Option<FloodProbes>)> {
    let components = &SETTINGS.components;
    let mut relays = (components.pumps.is_initialized() || components.flood.is_running())
        .then(|| pins.open())
        .transpose()?;
    let initial_flood = flood_inputs
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
    Ok((relays, initial_flood))
}

pub(super) type Heaters = Control<HeaterService<NvsHeaterSettings>>;
pub(super) type Pumps = Control<PumpService<PumpStorage, SystemClock>>;
pub(super) type Reservoir = Control<ReservoirService<ReservoirStorage, SystemClock>>;

impl Heaters {
    pub fn load(nvs_partition: Option<EspDefaultNvsPartition>) -> anyhow::Result<Self> {
        let service = HeaterService::new(
            SETTINGS.heaters.defaults,
            NvsHeaterSettings::new(nvs_partition)
                .context("Failed to open the heater namespace in NVS")?,
        );
        Ok(Self::new(service, SETTINGS.components.heaters.is_running()))
    }

    pub fn task(
        &self,
        pins: HeaterPins,
        store: &InMemoryTelemetryStore,
    ) -> anyhow::Result<(
        Option<impl Future<Output = ()>>,
        Option<SsrOutputs<'static>>,
    )> {
        let ssr = SETTINGS
            .components
            .heaters
            .is_initialized()
            .then(|| pins.open())
            .transpose()?;
        Ok(match (ssr, self.automated) {
            (Some(outputs), true) => {
                info!("Heater control running");
                (
                    Some(heater_loop(
                        HeaterController::new(outputs, Arc::clone(&self.service)),
                        store.subscribe_temperature(),
                        self.service.change_signal(),
                        Arc::clone(&self.republish),
                        SETTINGS.heaters.watchdog,
                        Arc::clone(&self.heartbeat),
                    )),
                    None,
                )
            }
            (Some(outputs), false) => {
                info!("Heater outputs held low but not controlled");
                (None, Some(outputs))
            }
            (None, _) => (None, None),
        })
    }
}

impl Pumps {
    pub fn load(
        nvs_partition: Option<EspDefaultNvsPartition>,
        clock: Arc<SystemClock>,
        pump_events: Option<UnboundedSender<OperationEvent>>,
        initial_flood: Option<FloodProbes>,
    ) -> Self {
        let components = &SETTINGS.components;
        let pump_nvs = nvs_partition.and_then(|partition| match NvsPumpStore::new(partition) {
            Ok(store) => Some(store),
            Err(error) => {
                error!(%error, "Pump NVS namespace unavailable, pump automation disabled");
                None
            }
        });
        let pumps_automated = components.pumps.is_running() && pump_nvs.is_some();
        let pumps = PumpService::new(
            SETTINGS.pumps.defaults.clone(),
            pump_nvs.map_or(
                PumpStorage::Unavailable {
                    flood_enabled: components.flood.is_running(),
                },
                PumpStorage::Nvs,
            ),
            clock,
            pump_events,
        );
        if let Some(probes) = initial_flood {
            pumps.observe_flood(probes);
        }
        Self::new(pumps, pumps_automated)
    }

    pub fn task(
        &self,
        relays: Option<RelayOutputs<'static>>,
        levels: &InMemoryWaterLevelStore,
    ) -> (
        Option<impl Future<Output = ()>>,
        Option<RelayOutputs<'static>>,
    ) {
        match (relays, self.automated) {
            (Some(outputs), true) => {
                info!("Pump control running");
                (
                    Some(pump_loop(
                        PumpController::new(
                            outputs,
                            Arc::clone(&self.service),
                            SETTINGS.pumps.timing,
                        ),
                        levels.subscribe(),
                        self.service.change_signal(),
                        Arc::clone(&self.republish),
                        SETTINGS.pumps.timing,
                        Arc::clone(&self.heartbeat),
                    )),
                    None,
                )
            }
            (Some(outputs), false) => {
                info!("Pump automation disabled");
                (None, Some(outputs))
            }
            (None, _) => (None, None),
        }
    }

    pub async fn watch_flood(
        &self,
        flood_inputs: Option<FloodInputs<'_>>,
        idle_relays: Option<RelayOutputs<'_>>,
        reservoir_changed: Arc<Notify>,
        flood_heartbeat: Arc<AtomicU32>,
    ) {
        let components = &SETTINGS.components;
        let pumps = &self.service;
        let mut idle = idle_relays
            .map(|outputs| PumpController::new(outputs, Arc::clone(pumps), SETTINGS.pumps.timing));
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
                self.republish.notify_one();
                reservoir_changed.notify_one();
                previous = Some(status);
            }
            sleep(Duration::from_millis(100)).await;
        }
    }
}

impl Reservoir {
    pub fn load(
        nvs_partition: Option<EspDefaultNvsPartition>,
        clock: Arc<SystemClock>,
        fill_events: Option<UnboundedSender<FillEvent>>,
    ) -> Self {
        let components = &SETTINGS.components;
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
        let reservoir = ReservoirService::new(
            SETTINGS.reservoir.defaults,
            reservoir_nvs.map_or(ReservoirStorage::Unavailable, ReservoirStorage::Nvs),
            clock,
            fill_events,
            SETTINGS.reservoir.timing,
        );
        Self::new(reservoir, reservoir_automated)
    }

    pub fn task(
        &self,
        barrel_relays: Option<ReservoirRelayOutputs<'static>>,
        levels: &InMemoryWaterLevelStore,
        store: &InMemoryTelemetryStore,
        pumps: &Pumps,
    ) -> (
        Option<impl Future<Output = ()>>,
        Option<ReservoirRelayOutputs<'static>>,
    ) {
        match (barrel_relays, self.automated) {
            (Some(outputs), true) => {
                info!("Barrel control running");
                (
                    Some(reservoir_loop(
                        ReservoirController::new(
                            outputs,
                            Arc::clone(&self.service),
                            SETTINGS.reservoir.timing,
                        ),
                        levels.subscribe(),
                        store.subscribe_tds(),
                        Arc::clone(&pumps.service),
                        pumps.automated.then(|| Arc::clone(&pumps.service)),
                        self.service.change_signal(),
                        Arc::clone(&self.republish),
                        SETTINGS.reservoir.timing,
                        Arc::clone(&self.heartbeat),
                    )),
                    None,
                )
            }
            (Some(outputs), false) => {
                info!("Barrel relays held released but not controlled");
                (None, Some(outputs))
            }
            (None, _) => (None, None),
        }
    }
}
