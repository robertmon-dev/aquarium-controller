use std::future::Future;
use std::sync::Arc;

use anyhow::Context;
use aquarium_core::adapters::outbound::memory::InMemoryWaterLevelStore;
use aquarium_core::application::{CalibrationService, TelemetryCollector, WaterLevelSampler};
use aquarium_core::ports::outbound::{
    CalibrationSource, Clock, HeaterStatusSource, LightingOverrideSource, PhReader,
    ProbeVoltageSink, PumpStatusSource, ReservoirStatusSource, SystemHealthProvider, TdsReader,
    TdsSampleSink, TelemetrySink, TemperatureReader, TemperatureSampleSink, WaterLevelPins,
    WaterLevelSink, WaterLevelSource,
};
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use tokio::sync::Notify;
use tokio::time::MissedTickBehavior;
use tracing::info;

use aquarium_controller::adapters::outbound::ds18b20::Ds18b20Adapter;
use aquarium_controller::adapters::outbound::i2c::ads1115::AnalogFrontEnd;
use aquarium_controller::adapters::outbound::i2c::mcp23017::Mcp23017;
use aquarium_controller::adapters::outbound::i2c::shared::SharedI2c;
use aquarium_controller::adapters::outbound::nvs_calibration::NvsCalibration;
use aquarium_controller::adapters::outbound::temperature::TemperatureSource;
use aquarium_controller::adapters::outbound::xkc_y25::Xkc25Adapter;

use crate::board::{I2cBus, TemperaturePin, LEVEL_SENSOR_PINS};
use crate::SETTINGS;

pub(super) fn temperature(pin: TemperaturePin) -> anyhow::Result<TemperatureSource<'static>> {
    let components = &SETTINGS.components;
    Ok(if components.temperature.is_initialized() {
        let bus = pin.open()?;
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
    })
}

pub(super) fn analog(bus: SharedI2c<'static>) -> Option<AnalogFrontEnd<'static>> {
    let components = &SETTINGS.components;

    if components.ph.is_initialized() || components.tds.is_initialized() {
        let front_end = I2cBus::open_analog(
            bus,
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
        );

        info!(
            ph = components.ph.is_running(),
            tds = components.tds.is_running(),
            "ADS1115 front end ready"
        );

        Some(front_end)
    } else {
        None
    }
}

pub(super) fn levels(
    expander: Mcp23017<'static>,
    levels: Arc<InMemoryWaterLevelStore>,
) -> Option<impl Future<Output = ()>> {
    let components = &SETTINGS.components;
    let sampler = if components.water_level.is_initialized() {
        let pins = Xkc25Adapter::new(expander, LEVEL_SENSOR_PINS);

        Some(WaterLevelSampler::new(
            pins,
            levels,
            SETTINGS.sensors.level.polarity,
            SETTINGS.sensors.level.filter,
        ))
    } else {
        None
    };

    match (sampler, components.water_level.is_running()) {
        (Some(sampler), true) => {
            info!("Water level sensors ready");
            Some(level_task(sampler))
        }
        (Some(_), false) => {
            info!("Water level inputs initialized but not sampling");
            None
        }
        (None, _) => None,
    }
}

pub(super) fn calibration(
    nvs_partition: Option<EspDefaultNvsPartition>,
) -> anyhow::Result<Arc<CalibrationService<NvsCalibration>>> {
    let calibration = Arc::new(CalibrationService::new(
        SETTINGS.sensors.calibration,
        NvsCalibration::new(nvs_partition)
            .context("Failed to open the calibration namespace in NVS")?,
    ));

    info!(
        makeup_tank_celsius = calibration.calibration().makeup_tank_celsius,
        tds_factor = calibration.calibration().tds_factor,
        "Calibration loaded"
    );
    Ok(calibration)
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

pub(super) async fn sensor_task<R, C, H, S, O, W, A, K, T, P, V>(
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
