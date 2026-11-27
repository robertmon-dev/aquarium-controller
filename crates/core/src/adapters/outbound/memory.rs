use std::sync::{Arc, Mutex};

use tokio::sync::watch;
use tokio::time::Instant;

use crate::domain::calibration::Calibration;
use crate::domain::heater::HeaterSettings;
use crate::domain::lighting::LightingPalette;
use crate::domain::metrics::{ProbeVoltagesDto, TankTelemetryDto};
use crate::domain::reservoir::ReservoirTelemetry;
use crate::domain::water::WaterLevelSnapshot;
use crate::ports::inbound::{ProbeVoltageQuery, TelemetryQuery};
use crate::ports::outbound::{
    CalibrationPersistence, HeaterPersistence, LeakSource, LevelSample, LightingPersistence,
    ProbeVoltageSink, PumpRecord, PumpStore, ReservoirRecord, ReservoirStatusSource,
    ReservoirStore, TdsSample, TdsSampleSink, TelemetrySink, TemperatureSample,
    TemperatureSampleSink, WaterLevelSink, WaterLevelSource,
};

pub struct InMemoryTelemetryStore {
    tx: watch::Sender<TankTelemetryDto>,
    temperature: watch::Sender<Option<TemperatureSample>>,
    tds: watch::Sender<Option<TdsSample>>,
    probes: Mutex<ProbeVoltagesDto>,
}

impl InMemoryTelemetryStore {
    pub fn new() -> Self {
        Self {
            tx: watch::channel(TankTelemetryDto::empty()).0,
            temperature: watch::channel(None).0,
            tds: watch::channel(None).0,
            probes: Mutex::new(ProbeVoltagesDto::NEVER_SAMPLED),
        }
    }

    pub fn subscribe_temperature(&self) -> watch::Receiver<Option<TemperatureSample>> {
        self.temperature.subscribe()
    }

    pub fn subscribe_tds(&self) -> watch::Receiver<Option<TdsSample>> {
        self.tds.subscribe()
    }
}

impl Default for InMemoryTelemetryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetrySink for InMemoryTelemetryStore {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.tx.send_replace(snapshot);
    }
}

impl TelemetryQuery for InMemoryTelemetryStore {
    fn snapshot(&self) -> TankTelemetryDto {
        self.tx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<TankTelemetryDto> {
        self.tx.subscribe()
    }
}

impl TelemetrySink for Arc<InMemoryTelemetryStore> {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.as_ref().publish(snapshot);
    }
}

impl TemperatureSampleSink for InMemoryTelemetryStore {
    fn publish_temperature(&self, sample: TemperatureSample) {
        self.temperature.send_replace(Some(sample));
    }
}

impl TemperatureSampleSink for Arc<InMemoryTelemetryStore> {
    fn publish_temperature(&self, sample: TemperatureSample) {
        self.as_ref().publish_temperature(sample);
    }
}

impl TdsSampleSink for InMemoryTelemetryStore {
    fn publish_tds(&self, sample: TdsSample) {
        self.tds.send_replace(Some(sample));
    }
}

impl TdsSampleSink for Arc<InMemoryTelemetryStore> {
    fn publish_tds(&self, sample: TdsSample) {
        self.as_ref().publish_tds(sample);
    }
}

impl ProbeVoltageSink for InMemoryTelemetryStore {
    fn publish_volts(&self, ph: Option<f32>, tds: Option<f32>, sampled_at_epoch_s: u64) {
        *self.probes.lock().expect("probe voltage mutex poisoned") = ProbeVoltagesDto {
            ph,
            tds,
            sampled_at_epoch_s,
        };
    }
}

impl ProbeVoltageSink for Arc<InMemoryTelemetryStore> {
    fn publish_volts(&self, ph: Option<f32>, tds: Option<f32>, sampled_at_epoch_s: u64) {
        self.as_ref().publish_volts(ph, tds, sampled_at_epoch_s);
    }
}

impl ProbeVoltageQuery for InMemoryTelemetryStore {
    fn probe_voltages(&self) -> ProbeVoltagesDto {
        *self.probes.lock().expect("probe voltage mutex poisoned")
    }
}

pub struct InMemoryWaterLevelStore {
    tx: watch::Sender<Option<LevelSample>>,
}

impl InMemoryWaterLevelStore {
    pub fn new() -> Self {
        Self {
            tx: watch::channel(None).0,
        }
    }

    pub fn subscribe(&self) -> watch::Receiver<Option<LevelSample>> {
        self.tx.subscribe()
    }
}

impl Default for InMemoryWaterLevelStore {
    fn default() -> Self {
        Self::new()
    }
}

impl WaterLevelSink for InMemoryWaterLevelStore {
    fn publish(&self, snapshot: WaterLevelSnapshot) {
        self.tx.send_replace(Some(LevelSample {
            levels: snapshot,
            measured_at: Instant::now(),
        }));
    }
}

impl WaterLevelSource for InMemoryWaterLevelStore {
    fn levels(&self) -> Option<LevelSample> {
        self.tx.borrow().clone()
    }
}

impl WaterLevelSink for Arc<InMemoryWaterLevelStore> {
    fn publish(&self, snapshot: WaterLevelSnapshot) {
        self.as_ref().publish(snapshot);
    }
}

impl WaterLevelSource for Arc<InMemoryWaterLevelStore> {
    fn levels(&self) -> Option<LevelSample> {
        self.as_ref().levels()
    }
}

pub struct VolatileCalibrationStore;

impl CalibrationPersistence for VolatileCalibrationStore {
    fn load(&self) -> Option<Calibration> {
        None
    }

    fn store(&self, _value: &Calibration) -> Result<(), &'static str> {
        Ok(())
    }
}

pub struct VolatileHeaterStore;

impl HeaterPersistence for VolatileHeaterStore {
    fn load(&self) -> Option<HeaterSettings> {
        None
    }

    fn store(&self, _value: &HeaterSettings) -> Result<(), &'static str> {
        Ok(())
    }
}

#[derive(Default)]
pub struct VolatileLightingStore {
    stored: Mutex<Option<LightingPalette>>,
}

impl LightingPersistence for VolatileLightingStore {
    fn load(&self) -> Option<LightingPalette> {
        *self.stored.lock().expect("lighting store mutex poisoned")
    }

    fn store(&self, value: &LightingPalette) -> Result<(), &'static str> {
        *self.stored.lock().expect("lighting store mutex poisoned") = Some(*value);
        Ok(())
    }
}

#[derive(Default)]
pub struct VolatilePumpStore {
    records: Mutex<[Option<Vec<u8>>; PumpRecord::COUNT]>,
}

impl VolatilePumpStore {
    pub fn get(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.records.lock().expect("pump store mutex poisoned")[record.index()].clone()
    }

    pub fn put(&self, record: PumpRecord, bytes: Vec<u8>) {
        self.records.lock().expect("pump store mutex poisoned")[record.index()] = Some(bytes);
    }
}

impl PumpStore for VolatilePumpStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.get(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.put(record, bytes.to_vec());
        Ok(())
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        self.records.lock().expect("pump store mutex poisoned")[record.index()] = None;
        Ok(())
    }
}

#[derive(Default)]
pub struct VolatileReservoirStore {
    records: Mutex<[Option<Vec<u8>>; ReservoirRecord::COUNT]>,
}

impl VolatileReservoirStore {
    pub fn get(&self, record: ReservoirRecord) -> Option<Vec<u8>> {
        self.records.lock().expect("reservoir store mutex poisoned")[record.index()].clone()
    }
}

impl ReservoirStore for VolatileReservoirStore {
    fn load(&self, record: ReservoirRecord) -> Result<Option<Vec<u8>>, &'static str> {
        Ok(self.get(record))
    }

    fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.records.lock().expect("reservoir store mutex poisoned")[record.index()] =
            Some(bytes.to_vec());
        Ok(())
    }

    fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str> {
        self.records.lock().expect("reservoir store mutex poisoned")[record.index()] = None;
        Ok(())
    }
}

pub struct NoLeakSensor;

impl LeakSource for NoLeakSensor {
    fn leak(&self) -> Option<bool> {
        None
    }
}

pub struct NoReservoir;

impl ReservoirStatusSource for NoReservoir {
    fn reservoir_telemetry(&self) -> ReservoirTelemetry {
        ReservoirTelemetry::UNAVAILABLE
    }
}

#[cfg(test)]
mod tests;
