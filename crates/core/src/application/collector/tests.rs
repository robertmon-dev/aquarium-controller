use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::{TelemetryCollector, TelemetryConfig, TelemetrySources};
use crate::adapters::outbound::memory::NoReservoir;
use crate::domain::calibration::{Calibration, PhCalibration};
use crate::domain::cron::CivilTime;
use crate::domain::heater::{HeaterMode, HeaterStatus, HeatersStatus};
use crate::domain::lighting::ChannelLevels;
use crate::domain::metrics::{
    LightingMetricsDto, LightingProfile, ProbeVoltagesDto, TankTelemetryDto, WaterTemperature,
};
use crate::domain::pump::{
    OperationKind, OperationRecord, OperationResult, OperationState, PumpsTelemetry, Supervision,
};
use crate::domain::reservoir::{FillPhase, ReservoirTelemetry};
use crate::domain::sump_light::SumpLightStatus;
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::outbound::{
    CalibrationSource, Clock, HeaterStatusSource, LevelSample, LightingStatusSource, PhReader,
    ProbeVoltageSink, PumpStatusSource, ReservoirStatusSource, SumpLightStatusSource,
    SystemHealthProvider, TdsReader, TdsSample, TdsSampleSink, TelemetrySink, TemperatureError,
    TemperatureReader, TemperatureSample, TemperatureSampleSink, TemperatureSensorRole,
    WaterLevelSource,
};
use crate::protocol::ph::{self, PhError};
use crate::protocol::tds::{self, TdsError};

struct FakeReader {
    tank: Option<f32>,
    sump: Option<f32>,
    trigger_fails: bool,
    conversions: usize,
}

impl FakeReader {
    fn healthy() -> Self {
        Self {
            tank: Some(26.5),
            sump: Some(26.0),
            trigger_fails: false,
            conversions: 0,
        }
    }
}

impl TemperatureReader for FakeReader {
    async fn trigger_conversion(&mut self) -> Result<(), TemperatureError> {
        self.conversions += 1;

        if self.trigger_fails {
            Err(TemperatureError::BusError("fake: the bus went down"))
        } else {
            Ok(())
        }
    }

    async fn read_temperature(
        &mut self,
        role: TemperatureSensorRole,
    ) -> Result<WaterTemperature, TemperatureError> {
        let value = match role {
            TemperatureSensorRole::MainTank => self.tank,
            TemperatureSensorRole::Sump => self.sump,
        };

        let celsius = value.ok_or(TemperatureError::SensorNotFound(role))?;
        WaterTemperature::new(celsius).map_err(TemperatureError::InvalidReading)
    }
}

struct FakeClock {
    epoch: Option<u64>,
    uptime: u64,
    minute: Option<u16>,
    month: Option<u8>,
}

impl FakeClock {
    fn synced_at_noon_in_june() -> Self {
        Self {
            epoch: Some(1_750_000_000),
            uptime: 3600,
            minute: Some(600),
            month: Some(6),
        }
    }

    fn unsynced() -> Self {
        Self {
            epoch: None,
            uptime: 12,
            minute: None,
            month: None,
        }
    }
}

impl Clock for FakeClock {
    fn epoch_seconds(&self) -> Option<u64> {
        self.epoch
    }
    fn uptime_seconds(&self) -> u64 {
        self.uptime
    }
    fn local_minute_of_day(&self) -> Option<u16> {
        self.minute
    }
    fn local_month(&self) -> Option<u8> {
        self.month
    }
    fn local_civil_time(&self) -> Option<CivilTime> {
        None
    }
}

struct FakeLighting(LightingMetricsDto);

impl FakeLighting {
    fn unavailable() -> Self {
        Self(LightingMetricsDto::UNAVAILABLE)
    }

    fn publishing(status: LightingMetricsDto) -> Self {
        Self(status)
    }
}

impl LightingStatusSource for FakeLighting {
    fn lighting_status(&self) -> LightingMetricsDto {
        self.0
    }
}

struct FakeLevels(Option<LevelSample>);

impl FakeLevels {
    fn sampled(levels: WaterLevelSnapshot) -> Self {
        Self(Some(LevelSample {
            levels,
            measured_at: Instant::now(),
        }))
    }

    fn never_sampled() -> Self {
        Self(None)
    }

    fn stale(levels: WaterLevelSnapshot) -> Self {
        Self(Some(LevelSample {
            levels,
            measured_at: Instant::now() - LEVEL_STALE,
        }))
    }

    fn all_dry() -> Self {
        Self::sampled(WaterLevelSnapshot::dry())
    }

    fn wet(roles: &[WaterLevelRole]) -> Self {
        let mut snapshot = WaterLevelSnapshot::dry();
        for role in roles {
            snapshot.sensors[role.index()].state = WaterLevelState::Wet;
        }
        Self::sampled(snapshot)
    }

    fn faulty(role: WaterLevelRole) -> Self {
        let mut snapshot = WaterLevelSnapshot::dry();
        snapshot.sensors[role.index()].faulty = true;
        Self::sampled(snapshot)
    }
}

impl WaterLevelSource for FakeLevels {
    fn levels(&self) -> Option<LevelSample> {
        self.0.clone()
    }
}

struct FakeAnalog {
    ph_volts: Option<f32>,
    tds_volts: Option<f32>,
}

impl FakeAnalog {
    fn nothing_attached() -> Self {
        Self {
            ph_volts: None,
            tds_volts: None,
        }
    }

    fn ph_at(volts: f32) -> Self {
        Self {
            ph_volts: Some(volts),
            tds_volts: None,
        }
    }

    fn tds_at(volts: f32) -> Self {
        Self {
            ph_volts: None,
            tds_volts: Some(volts),
        }
    }
}

impl PhReader for FakeAnalog {
    async fn read_volts(&mut self) -> Result<f32, PhError> {
        self.ph_volts
            .ok_or(PhError::Unavailable("fake: no pH probe attached"))
    }
}

impl TdsReader for FakeAnalog {
    async fn read_volts(&mut self) -> Result<f32, TdsError> {
        self.tds_volts
            .ok_or(TdsError::Unavailable("fake: no TDS probe attached"))
    }
}

struct FakeCalibration(Calibration);

impl FakeCalibration {
    fn standard() -> Self {
        Self(Calibration {
            ph: CAL,
            makeup_tank_celsius: 25.0,
            tds_factor: 1.0,
        })
    }

    fn with(mut self, change: impl FnOnce(&mut Calibration)) -> Self {
        change(&mut self.0);
        self
    }
}

impl CalibrationSource for FakeCalibration {
    fn calibration(&self) -> Calibration {
        self.0
    }
}

struct FakePumps(PumpsTelemetry, Option<OperationRecord>);

impl PumpStatusSource for FakePumps {
    fn pump_telemetry(&self) -> PumpsTelemetry {
        self.0
    }

    fn last_top_off(&self) -> Option<OperationRecord> {
        self.1
    }
}

struct FakeHeaters(HeatersStatus);

impl HeaterStatusSource for FakeHeaters {
    fn heater_status(&self) -> HeatersStatus {
        self.0
    }
}

const CAL: PhCalibration = PhCalibration {
    neutral_volts: 1.500,
    acid_volts: 1.67748,
    calibrated_at_celsius: 25.0,
};

const NOMINAL_VOLTS_PER_PH: f32 = 0.05916;

const LEVEL_STALE: Duration = Duration::from_secs(2);

fn config() -> TelemetryConfig {
    TelemetryConfig {
        target_celsius: 27.0,
        ph_nominal_volts_per_ph: NOMINAL_VOLTS_PER_PH,
        level_stale: LEVEL_STALE,
    }
}

struct FakeHealth;

impl SystemHealthProvider for FakeHealth {
    fn wifi_rssi_dbm(&self) -> Option<i8> {
        Some(-57)
    }
    fn free_heap_kb(&self) -> u32 {
        180
    }
}

struct OfflineHealth;

impl SystemHealthProvider for OfflineHealth {
    fn wifi_rssi_dbm(&self) -> Option<i8> {
        None
    }
    fn free_heap_kb(&self) -> u32 {
        180
    }
}

#[derive(Default)]
struct RecordingSink {
    snapshots: Mutex<Vec<TankTelemetryDto>>,
    volts: Mutex<Vec<ProbeVoltagesDto>>,
    temperatures: Mutex<Vec<TemperatureSample>>,
    tds: Mutex<Vec<TdsSample>>,
}

impl RecordingSink {
    fn last(&self) -> TankTelemetryDto {
        self.snapshots
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the sink received nothing")
    }

    fn count(&self) -> usize {
        self.snapshots.lock().unwrap().len()
    }

    fn last_volts(&self) -> ProbeVoltagesDto {
        self.volts
            .lock()
            .unwrap()
            .last()
            .copied()
            .expect("the sink received no voltages")
    }

    fn volts_count(&self) -> usize {
        self.volts.lock().unwrap().len()
    }
}

impl TelemetrySink for &RecordingSink {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.snapshots.lock().unwrap().push(snapshot);
    }
}

impl TemperatureSampleSink for &RecordingSink {
    fn publish_temperature(&self, sample: TemperatureSample) {
        self.temperatures.lock().unwrap().push(sample);
    }
}

impl TdsSampleSink for &RecordingSink {
    fn publish_tds(&self, sample: TdsSample) {
        self.tds.lock().unwrap().push(sample);
    }
}

impl ProbeVoltageSink for &RecordingSink {
    fn publish_volts(&self, ph: Option<f32>, tds: Option<f32>, sampled_at_epoch_s: u64) {
        self.volts.lock().unwrap().push(ProbeVoltagesDto {
            ph,
            tds,
            sampled_at_epoch_s,
        });
    }
}

type Subject<'a> = TelemetryCollector<
    FakeReader,
    FakeClock,
    FakeHealth,
    &'a RecordingSink,
    FakeLighting,
    FakeLevels,
    FakeAnalog,
    FakeCalibration,
    FakeHeaters,
    FakePumps,
    NoReservoir,
    FakeSumpLight,
>;

fn collector(reader: FakeReader, clock: FakeClock, sink: &RecordingSink) -> Subject<'_> {
    build(
        reader,
        clock,
        sink,
        FakeLighting::unavailable(),
        FakeLevels::all_dry(),
    )
}

fn with_levels(sink: &RecordingSink, levels: FakeLevels) -> Subject<'_> {
    build(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        sink,
        FakeLighting::unavailable(),
        levels,
    )
}

fn build(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    lighting: FakeLighting,
    levels: FakeLevels,
) -> Subject<'_> {
    with_analog(
        reader,
        clock,
        sink,
        lighting,
        levels,
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
}

fn with_analog(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    lighting: FakeLighting,
    levels: FakeLevels,
    analog: FakeAnalog,
    calibration: FakeCalibration,
) -> Subject<'_> {
    TelemetryCollector::new(
        TelemetrySources {
            temperature: reader,
            clock,
            health: FakeHealth,
            lighting,
            levels,
            analog,
            calibration,
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
            reservoir: NoReservoir,
            sump_light: FakeSumpLight(SumpLightStatus::UNAVAILABLE),
        },
        sink,
        config(),
    )
}

fn analog_subject(
    sink: &RecordingSink,
    reader: FakeReader,
    analog: FakeAnalog,
    calibration: FakeCalibration,
) -> Subject<'_> {
    with_analog(
        reader,
        FakeClock::synced_at_noon_in_june(),
        sink,
        FakeLighting::unavailable(),
        FakeLevels::all_dry(),
        analog,
        calibration,
    )
}

struct FakeSumpLight(SumpLightStatus);

impl SumpLightStatusSource for FakeSumpLight {
    fn sump_light_status(&self) -> SumpLightStatus {
        self.0
    }
}

fn build_with_sump_light(sump_light: FakeSumpLight, sink: &RecordingSink) -> Subject<'_> {
    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            lighting: FakeLighting::unavailable(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
            reservoir: NoReservoir,
            sump_light,
        },
        sink,
        config(),
    )
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected ~{expected}, got {actual}"
    );
}

mod actuators;
mod levels;
mod lighting;
mod ph_probe;
mod probe_voltages;
mod system_health;
mod tds_probe;
mod temperature;
