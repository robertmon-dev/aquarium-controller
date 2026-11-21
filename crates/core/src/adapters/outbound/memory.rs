use std::sync::{Arc, Mutex};

use tokio::sync::{watch, Notify};
use tokio::time::Instant;

use crate::domain::calibration::Calibration;
use crate::domain::heater::HeaterSettings;
use crate::domain::light::{LightingOverride, LightingTarget};
use crate::domain::metrics::{ProbeVoltagesDto, TankTelemetryDto};
use crate::domain::water::WaterLevelSnapshot;
use crate::ports::inbound::{LightingControl, ProbeVoltageQuery, TelemetryQuery};
use crate::ports::outbound::{
    CalibrationPersistence, HeaterPersistence, LevelSample, LightingOverrideSource,
    ProbeVoltageSink, PumpRecord, PumpStore, TelemetrySink, TemperatureSample,
    TemperatureSampleSink, WaterLevelSink, WaterLevelSource,
};

pub struct InMemoryTelemetryStore {
    tx: watch::Sender<TankTelemetryDto>,
    temperature: watch::Sender<Option<TemperatureSample>>,
    probes: Mutex<ProbeVoltagesDto>,
}

impl InMemoryTelemetryStore {
    pub fn new() -> Self {
        Self {
            tx: watch::channel(TankTelemetryDto::empty()).0,
            temperature: watch::channel(None).0,
            probes: Mutex::new(ProbeVoltagesDto::NEVER_SAMPLED),
        }
    }

    pub fn subscribe_temperature(&self) -> watch::Receiver<Option<TemperatureSample>> {
        self.temperature.subscribe()
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

struct StoredOverride {
    target: LightingTarget,
    expires_at: Option<Instant>,
}

pub struct InMemoryLightingStore {
    state: Mutex<Option<StoredOverride>>,
    notify: Arc<Notify>,
}

impl InMemoryLightingStore {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    fn read(&self) -> Option<LightingOverride> {
        let guard = self.state.lock().expect("mutex poisoned by a panic");
        let stored = guard.as_ref()?;

        let expires_in_s = match stored.expires_at {
            None => None,
            Some(at) => {
                let now = Instant::now();
                if now >= at {
                    return None;
                }
                Some(at.saturating_duration_since(now).as_secs() as u32)
            }
        };

        Some(LightingOverride {
            target: stored.target,
            expires_in_s,
        })
    }
}

impl Default for InMemoryLightingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LightingControl for InMemoryLightingStore {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        let expires_at =
            duration_s.map(|s| Instant::now() + std::time::Duration::from_secs(s as u64));

        *self.state.lock().expect("mutex poisoned by a panic") =
            Some(StoredOverride { target, expires_at });

        self.notify.notify_one();
    }

    fn clear_override(&self) {
        *self.state.lock().expect("mutex poisoned by a panic") = None;
        self.notify.notify_one();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingOverrideSource for InMemoryLightingStore {
    fn active_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingControl for Arc<InMemoryLightingStore> {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        self.as_ref().set_override(target, duration_s);
    }

    fn clear_override(&self) {
        self.as_ref().clear_override();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.as_ref().current_override()
    }
}

impl LightingOverrideSource for Arc<InMemoryLightingStore> {
    fn active_override(&self) -> Option<LightingOverride> {
        self.as_ref().active_override()
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

#[cfg(test)]
mod tests;
