use anyhow::Context;
use aquarium_controller::adapters::outbound::influx::InfluxWriter;
use aquarium_controller::infrastructure::wifi::WifiManager;
use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;
use aquarium_core::application::{export_loop, MetricsExporter};
use aquarium_core::domain::pump::OperationEvent;
use aquarium_core::domain::reservoir::FillEvent;
use aquarium_core::ports::inbound::TelemetryQuery;
use esp_idf_hal::modem::Modem;
use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use std::future::Future;
use tokio::sync::mpsc;
use tracing::info;

use crate::SETTINGS;

pub(super) fn wifi(
    modem: Modem<'static>,
    nvs_partition: Option<EspDefaultNvsPartition>,
) -> anyhow::Result<Option<impl Future<Output = ()>>> {
    let components = &SETTINGS.components;
    let wifi = if components.wifi.is_initialized() {
        let sys_loop = EspSystemEventLoop::take().context("System event loop already taken")?;

        Some(
            WifiManager::new(modem, sys_loop, nvs_partition, &SETTINGS.wifi)
                .context("Failed to initialize WiFi")?,
        )
    } else {
        None
    };
    Ok(match (wifi, components.wifi.is_running()) {
        (Some(wifi), true) => Some(wifi.run()),
        (Some(_), false) => {
            info!("WiFi initialized but not connecting");
            None
        }
        (None, _) => None,
    })
}

pub(super) fn export_events<T>() -> (
    Option<mpsc::UnboundedSender<T>>,
    Option<mpsc::UnboundedReceiver<T>>,
) {
    if SETTINGS.influx.is_some() && SETTINGS.components.influx.is_running() {
        let (sender, receiver) = mpsc::unbounded_channel();
        (Some(sender), Some(receiver))
    } else {
        (None, None)
    }
}

pub(super) fn export(
    store: &InMemoryTelemetryStore,
    pump_events_rx: Option<mpsc::UnboundedReceiver<OperationEvent>>,
    fill_events_rx: Option<mpsc::UnboundedReceiver<FillEvent>>,
) -> anyhow::Result<Option<impl Future<Output = ()>>> {
    let components = &SETTINGS.components;
    Ok(
        match (&SETTINGS.influx, components.influx.is_initialized()) {
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
                        pump_events_rx
                            .context("pump events are not wired for the Influx export")?,
                        fill_events_rx
                            .context("fill events are not wired for the Influx export")?,
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
        },
    )
}
