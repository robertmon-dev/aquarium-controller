use std::sync::Mutex;
use std::time::Duration;

use tokio::time::Instant;

use super::{TelemetryCollector, TelemetryConfig, TelemetrySources};
use crate::domain::calibration::{Calibration, PhCalibration};
use crate::domain::cron::CivilTime;
use crate::domain::heater::{HeaterMode, HeaterStatus, HeatersStatus};
use crate::domain::light::{ChannelLevels, LightingOverride, LightingTarget};
use crate::domain::metrics::{
    LightingProfile, ProbeVoltagesDto, TankTelemetryDto, WaterTemperature,
};
use crate::domain::pump::{
    OperationKind, OperationRecord, OperationResult, OperationState, PumpsTelemetry, Supervision,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::outbound::{
    CalibrationSource, Clock, HeaterStatusSource, LevelSample, LightingOverrideSource, PhReader,
    ProbeVoltageSink, PumpStatusSource, SystemHealthProvider, TdsReader, TdsSample, TdsSampleSink,
    TelemetrySink, TemperatureError, TemperatureReader, TemperatureSample, TemperatureSampleSink,
    TemperatureSensorRole, WaterLevelSource,
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

struct FakeOverrides(Option<LightingOverride>);

impl FakeOverrides {
    fn none() -> Self {
        Self(None)
    }

    fn forcing(target: LightingTarget) -> Self {
        Self(Some(LightingOverride {
            target,
            expires_in_s: None,
        }))
    }
}

impl LightingOverrideSource for FakeOverrides {
    fn active_override(&self) -> Option<LightingOverride> {
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
    FakeOverrides,
    FakeLevels,
    FakeAnalog,
    FakeCalibration,
    FakeHeaters,
    FakePumps,
>;

fn collector(reader: FakeReader, clock: FakeClock, sink: &RecordingSink) -> Subject<'_> {
    build(
        reader,
        clock,
        sink,
        FakeOverrides::none(),
        FakeLevels::all_dry(),
    )
}

fn with_overrides(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    overrides: FakeOverrides,
) -> Subject<'_> {
    build(reader, clock, sink, overrides, FakeLevels::all_dry())
}

fn with_levels(sink: &RecordingSink, levels: FakeLevels) -> Subject<'_> {
    build(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        sink,
        FakeOverrides::none(),
        levels,
    )
}

fn build(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    overrides: FakeOverrides,
    levels: FakeLevels,
) -> Subject<'_> {
    with_analog(
        reader,
        clock,
        sink,
        overrides,
        levels,
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
}

fn with_analog(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    overrides: FakeOverrides,
    levels: FakeLevels,
    analog: FakeAnalog,
    calibration: FakeCalibration,
) -> Subject<'_> {
    TelemetryCollector::new(
        TelemetrySources {
            temperature: reader,
            clock,
            health: FakeHealth,
            overrides,
            levels,
            analog,
            calibration,
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
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
        FakeOverrides::none(),
        FakeLevels::all_dry(),
        analog,
        calibration,
    )
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.01,
        "expected ~{expected}, got {actual}"
    );
}

#[tokio::test]
async fn publishes_both_sensor_readings() {
    let sink = RecordingSink::default();
    collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.temperature.tank_celsius, Some(26.5));
    assert_eq!(s.temperature.sump_celsius, Some(26.0));
    assert!(s.temperature.sensors_healthy);
}

#[tokio::test]
async fn one_dead_sensor_does_not_lose_the_other() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(26.5),
        sump: None,
        trigger_fails: false,
        conversions: 0,
    };
    collector(reader, FakeClock::synced_at_noon_in_june(), &sink)
        .sample()
        .await;

    let s = sink.last();
    assert_eq!(s.temperature.tank_celsius, Some(26.5));
    assert_eq!(s.temperature.sump_celsius, None);
    assert!(!s.temperature.sensors_healthy);
}

#[tokio::test]
async fn bus_failure_still_publishes_a_snapshot() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(26.5),
        sump: Some(26.0),
        trigger_fails: true,
        conversions: 0,
    };
    collector(reader, FakeClock::synced_at_noon_in_june(), &sink)
        .sample()
        .await;

    assert_eq!(sink.count(), 1, "a bus failure must not stall telemetry");
    let s = sink.last();
    assert_eq!(s.temperature.tank_celsius, None);
    assert_eq!(s.temperature.sump_celsius, None);
    assert_eq!(
        s.system_health.free_heap_kb, 180,
        "system health keeps working"
    );
}

#[tokio::test]
async fn carries_system_health_and_timestamp() {
    let sink = RecordingSink::default();
    collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.timestamp_epoch_s, 1_750_000_000);
    assert_eq!(s.system_health.uptime_seconds, 3600);
    assert_eq!(s.system_health.wifi_rssi_dbm, Some(-57));
}

#[tokio::test]
async fn an_unreadable_wifi_signal_is_reported_as_absent_not_zero() {
    let sink = RecordingSink::default();

    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: OfflineHealth,
            overrides: FakeOverrides::none(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
        },
        &sink,
        config(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().system_health.wifi_rssi_dbm, None);
}

#[tokio::test]
async fn evaluates_lighting_from_the_clock() {
    let sink = RecordingSink::default();
    collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.lighting.profile, LightingProfile::Daylight);
    assert_eq!(s.lighting.channel_white_pct, 60);
}

#[tokio::test]
async fn unsynced_clock_reports_zero_timestamp_and_lights_off() {
    let sink = RecordingSink::default();
    collector(FakeReader::healthy(), FakeClock::unsynced(), &sink)
        .sample()
        .await;

    let s = sink.last();
    assert_eq!(s.timestamp_epoch_s, 0, "0 is a sentinel, not a faked date");
    assert_eq!(s.system_health.uptime_seconds, 12, "uptime is always known");
    assert_eq!(s.lighting.profile, LightingProfile::Off);
    assert_eq!(s.lighting.channel_white_pct, 0);
}

#[tokio::test]
async fn out_of_safe_range_reading_is_reported_not_dropped() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(21.0),
        sump: Some(26.0),
        trigger_fails: false,
        conversions: 0,
    };
    collector(reader, FakeClock::synced_at_noon_in_june(), &sink)
        .sample()
        .await;

    assert_eq!(
        sink.last().temperature.tank_celsius,
        Some(21.0),
        "an unsafe temperature MUST reach telemetry"
    );
}

#[tokio::test]
async fn an_override_beats_the_schedule() {
    let sink = RecordingSink::default();
    with_overrides(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
        FakeOverrides::forcing(LightingTarget::Profile(LightingProfile::Off)),
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(
        s.lighting.profile,
        LightingProfile::Off,
        "at noon the schedule wants Daylight, but the override wins"
    );
    assert_eq!(s.lighting.channel_white_pct, 0);
    assert!(s.lighting.override_active);
}

#[tokio::test]
async fn without_an_override_the_schedule_still_rules() {
    let sink = RecordingSink::default();
    collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.lighting.profile, LightingProfile::Daylight);
    assert!(!s.lighting.override_active);
}

#[tokio::test]
async fn a_channels_override_applies_raw_levels() {
    let sink = RecordingSink::default();
    with_overrides(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
        FakeOverrides::forcing(LightingTarget::Channels(ChannelLevels::new(80, 20))),
    )
    .sample()
    .await;

    let s = sink.last();
    assert_eq!(s.lighting.channel_white_pct, 80);
    assert_eq!(s.lighting.channel_rgb_pct, 20);
    assert!(s.lighting.override_active);
}

#[tokio::test]
async fn republish_does_not_touch_the_sensor_bus() {
    let sink = RecordingSink::default();
    let mut c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );

    c.sample().await;
    let after_sample = c.sources.temperature.conversions;

    c.republish();
    c.republish();

    assert_eq!(sink.count(), 3, "every call publishes a snapshot");
    assert_eq!(
        c.sources.temperature.conversions, after_sample,
        "a burst of API requests MUST NOT hammer the 1-Wire bus"
    );
}

#[tokio::test]
async fn republish_reuses_the_last_readings() {
    let sink = RecordingSink::default();
    let mut c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );

    c.sample().await;
    c.republish();

    assert_eq!(sink.last().temperature.tank_celsius, Some(26.5));
}

#[tokio::test(start_paused = true)]
async fn only_a_sample_publishes_temperatures_stamped_when_measured() {
    let sink = RecordingSink::default();
    let mut c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );
    let measured_at = tokio::time::Instant::now();

    c.sample().await;
    tokio::time::advance(std::time::Duration::from_millis(1_900)).await;
    c.republish();

    assert_eq!(
        *sink.temperatures.lock().unwrap(),
        vec![TemperatureSample {
            tank_celsius: Some(26.5),
            sump_celsius: Some(26.0),
            measured_at,
        }]
    );
}

#[tokio::test]
async fn republish_before_any_sample_reports_no_readings() {
    let sink = RecordingSink::default();
    let c = collector(
        FakeReader::healthy(),
        FakeClock::synced_at_noon_in_june(),
        &sink,
    );

    c.republish();

    let s = sink.last();
    assert_eq!(
        s.temperature.tank_celsius, None,
        "an empty cache reports no reading instead of faking data"
    );
    assert!(!s.temperature.sensors_healthy);
}

#[tokio::test]
async fn publishes_the_water_levels_from_the_source() {
    let sink = RecordingSink::default();
    with_levels(
        &sink,
        FakeLevels::wet(&[WaterLevelRole::SumpPump, WaterLevelRole::MainTank]),
    )
    .sample()
    .await;

    let s = sink.last();
    assert!(s.water_level.sump_pump.wet);
    assert!(s.water_level.main_tank.wet);
    assert!(!s.water_level.sump_mechanical.wet);
    assert!(!s.water_level.sump_biological.wet);
    assert!(s.water_level.sensors_healthy);
}

#[tokio::test]
async fn a_faulty_level_sensor_reaches_telemetry() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::faulty(WaterLevelRole::SumpMechanical))
        .sample()
        .await;

    let s = sink.last();
    assert!(s.water_level.sump_mechanical.faulty);
    assert!(!s.water_level.sensors_healthy);
}

#[tokio::test]
async fn the_sump_is_low_for_the_ato_when_the_pump_chamber_is_dry() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::all_dry()).sample().await;

    assert!(sink.last().ato.sump_water_low);
}

#[tokio::test]
async fn the_sump_is_not_low_for_the_ato_while_the_pump_chamber_is_wet() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::SumpPump]))
        .sample()
        .await;

    let s = sink.last();
    assert!(!s.ato.sump_water_low);
    assert!(!s.ato.pump_active);
}

#[tokio::test]
async fn levels_before_the_first_sample_are_reported_as_unavailable() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::never_sampled())
        .sample()
        .await;

    let s = sink.last();
    assert!(!s.water_level.available);
    assert!(!s.water_level.sensors_healthy);
    assert!(!s.water_level.sump_pump.wet);
    assert!(!s.ato.sump_water_low);
    assert!(!s.ato.reservoir_empty);
}

#[tokio::test]
async fn a_real_all_dry_reading_is_available_and_drives_the_ato_flags() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::all_dry()).sample().await;

    let s = sink.last();
    assert!(s.water_level.available);
    assert!(s.water_level.sensors_healthy);
    assert!(s.ato.sump_water_low);
    assert!(s.ato.reservoir_empty);
}

#[tokio::test]
async fn levels_from_a_stopped_sampler_are_reported_as_unavailable() {
    let sink = RecordingSink::default();
    with_levels(
        &sink,
        FakeLevels::stale(
            FakeLevels::wet(&[WaterLevelRole::SumpPump])
                .0
                .unwrap()
                .levels,
        ),
    )
    .sample()
    .await;

    let s = sink.last();
    assert!(!s.water_level.available);
    assert!(!s.water_level.sensors_healthy);
    assert!(!s.water_level.sump_pump.wet);
    assert!(!s.ato.sump_water_low);
}

#[tokio::test]
async fn a_filled_reservoir_is_not_empty_for_the_ato() {
    let sink = RecordingSink::default();
    with_levels(
        &sink,
        FakeLevels::wet(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]),
    )
    .sample()
    .await;

    assert!(!sink.last().ato.reservoir_empty);
}

async fn ato_with(
    pumps: PumpsTelemetry,
    last_top_off: Option<OperationRecord>,
) -> TankTelemetryDto {
    let sink = RecordingSink::default();
    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            overrides: FakeOverrides::none(),
            levels: FakeLevels::wet(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(pumps, last_top_off),
        },
        &sink,
        config(),
    )
    .sample()
    .await;
    sink.last()
}

fn top_off_record(refill_ran_s: u32) -> OperationRecord {
    OperationRecord {
        kind: OperationKind::TopOff,
        started_at: Some(1_750_000_000),
        drain_ran_s: 0,
        settle_ran_s: 0,
        refill_ran_s,
        result: OperationResult::Completed,
        resumed: false,
        finished_at: Some(1_750_000_000 + u64::from(refill_ran_s)),
    }
}

#[tokio::test]
async fn the_ato_card_follows_a_running_top_off_and_the_last_fill() {
    let topping_off = PumpsTelemetry {
        makeup: true,
        operation_kind: Some(OperationKind::TopOff),
        operation_state: OperationState::Refilling,
        ..PumpsTelemetry::UNAVAILABLE
    };

    let s = ato_with(topping_off, Some(top_off_record(42))).await;
    assert!(s.ato.pump_active);
    assert_eq!(s.ato.last_fill_duration_ms, 42_000);

    let interlocked = PumpsTelemetry {
        makeup: false,
        ..topping_off
    };
    assert!(!ato_with(interlocked, None).await.ato.pump_active);
    assert_eq!(
        ato_with(interlocked, None).await.ato.last_fill_duration_ms,
        0
    );
}

#[tokio::test]
async fn a_water_change_refill_is_not_an_ato_top_off() {
    let refilling = PumpsTelemetry {
        makeup: true,
        operation_kind: Some(OperationKind::WaterChange),
        operation_state: OperationState::Refilling,
        ..PumpsTelemetry::UNAVAILABLE
    };

    let s = ato_with(refilling, Some(top_off_record(42))).await;
    assert!(!s.ato.pump_active);
    assert_eq!(s.ato.last_fill_duration_ms, 42_000);
}

#[tokio::test]
async fn the_reserve_chamber_does_not_drive_the_ato_low_flag() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::SumpPump]))
        .sample()
        .await;

    assert!(!sink.last().ato.sump_water_low);
    assert!(!sink.last().water_level.sump_reserve.wet);
}

#[tokio::test]
async fn republish_keeps_the_last_water_levels() {
    let sink = RecordingSink::default();
    let mut subject = with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::MainTank]));

    subject.sample().await;
    subject.republish();

    let s = sink.last();
    assert!(s.water_level.main_tank.wet);
}

#[tokio::test]
async fn publishes_ph_converted_from_the_probe_voltage() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(25.0),
        sump: Some(25.0),
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let expected = ph::convert(CAL.acid_volts, 25.0, &CAL)
        .expect("valid")
        .value();
    assert_close(sink.last().co2.current_ph.expect("a reading"), expected);
}

#[tokio::test]
async fn an_unavailable_probe_reports_no_ph_rather_than_a_stand_in() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().co2.current_ph, None);
}

#[tokio::test]
async fn ph_compensation_uses_the_sump_temperature_not_the_tank() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(60.0),
        sump: Some(25.0),
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let from_sump = ph::convert(CAL.acid_volts, 25.0, &CAL)
        .expect("valid")
        .value();
    let from_tank = ph::convert(CAL.acid_volts, 60.0, &CAL)
        .expect("valid")
        .value();

    let reported = sink.last().co2.current_ph.expect("a reading");
    assert_close(reported, from_sump);
    assert!(
        (reported - from_tank).abs() > 0.1,
        "the pH probe sits in the sump, so the tank temperature must not drive compensation"
    );
}

#[tokio::test]
async fn without_a_sump_reading_ph_compensation_falls_back_to_the_target() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(26.0),
        sump: None,
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let expected = ph::convert(CAL.acid_volts, config().target_celsius, &CAL)
        .expect("valid")
        .value();

    assert_close(sink.last().co2.current_ph.expect("a reading"), expected);
}

#[tokio::test]
async fn reports_electrode_wear_against_the_nominal_slope() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(CAL.neutral_volts),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let health = sink.last().co2.ph_electrode_health.expect("a ratio");
    assert_close(health, 1.0);
}

#[tokio::test]
async fn a_worn_electrode_reports_a_ratio_below_one() {
    let sink = RecordingSink::default();
    let worn = PhCalibration {
        neutral_volts: 1.500,
        acid_volts: 1.650,
        calibrated_at_celsius: 25.0,
    };

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(1.5),
        FakeCalibration::standard().with(|cal| cal.ph = worn),
    )
    .sample()
    .await;

    let health = sink.last().co2.ph_electrode_health.expect("a ratio");
    assert!(
        health > 0.8 && health < 0.9,
        "150 mV over three pH units is about 85% of nominal, got {health}"
    );
}

#[tokio::test]
async fn republish_keeps_the_last_ph_reading() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(CAL.acid_volts),
        FakeCalibration::standard(),
    );

    subject.sample().await;
    subject.republish();

    assert!(sink.last().co2.current_ph.is_some());
}

#[tokio::test]
async fn a_tds_reading_reaches_telemetry_in_ppm_and_conductivity() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(2.16),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let expected = tds::convert(2.16, 25.0, 1.0).expect("valid");
    let published = sink.last();

    assert_close(published.tds.ppm.expect("a reading"), expected.ppm.as_ppm());
    assert_close(
        published.tds.ec_us_cm.expect("a reading"),
        expected.ec_us_cm,
    );
}

#[tokio::test]
async fn an_unavailable_tds_probe_reports_nothing_rather_than_a_stand_in() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().tds.ppm, None);
    assert_eq!(sink.last().tds.ec_us_cm, None);
}

#[tokio::test]
async fn tds_compensation_uses_the_makeup_tank_temperature_from_calibration() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(1.0),
        FakeCalibration::standard().with(|cal| cal.makeup_tank_celsius = 15.0),
    )
    .sample()
    .await;

    let expected = tds::convert(1.0, 15.0, 1.0).expect("valid");
    let at_reference = tds::convert(1.0, 25.0, 1.0).expect("valid");
    let reported = sink.last().tds.ppm.expect("a reading");

    assert_close(reported, expected.ppm.as_ppm());
    assert!(
        (reported - at_reference.ppm.as_ppm()).abs() > 1.0,
        "the makeup tank temperature from calibration must drive compensation, \
         not the 25 C reference"
    );
}

#[tokio::test]
async fn the_tds_factor_from_calibration_scales_the_reading() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(1.0),
        FakeCalibration::standard().with(|cal| cal.tds_factor = 1.5),
    )
    .sample()
    .await;

    let expected = tds::convert(1.0, 25.0, 1.5).expect("valid");

    assert_close(
        sink.last().tds.ppm.expect("a reading"),
        expected.ppm.as_ppm(),
    );
}

#[tokio::test]
async fn a_dead_ph_probe_does_not_suppress_the_tds_reading() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(1.5),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let published = sink.last();

    assert_eq!(published.co2.current_ph, None);
    assert!(published.tds.ppm.is_some());
}

#[tokio::test]
async fn ph_calibration_comes_from_the_port_not_the_static_config() {
    let sink = RecordingSink::default();
    let shifted = PhCalibration {
        neutral_volts: 1.400,
        acid_volts: 1.57748,
        calibrated_at_celsius: 25.0,
    };

    let reader = FakeReader {
        tank: Some(25.0),
        sump: Some(25.0),
        trigger_fails: false,
        conversions: 0,
    };

    analog_subject(
        &sink,
        reader,
        FakeAnalog::ph_at(shifted.neutral_volts),
        FakeCalibration::standard().with(|cal| cal.ph = shifted),
    )
    .sample()
    .await;

    assert_close(sink.last().co2.current_ph.expect("a reading"), 7.0);
}

#[tokio::test]
async fn sampling_publishes_the_raw_probe_voltages() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog {
            ph_volts: Some(1.52),
            tds_volts: Some(0.91),
        },
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let volts = sink.last_volts();

    assert_close(volts.ph.expect("a pH voltage"), 1.52);
    assert_close(volts.tds.expect("a TDS voltage"), 0.91);
}

#[tokio::test]
async fn the_published_voltages_carry_the_same_timestamp_as_the_snapshot() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(1.5),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(
        sink.last_volts().sampled_at_epoch_s,
        sink.last().timestamp_epoch_s,
        "a client comparing the two would otherwise see the voltages as stale or from the future"
    );
}

#[tokio::test]
async fn a_probe_that_failed_publishes_no_voltage_rather_than_zero() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(0.91),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    let volts = sink.last_volts();

    assert_eq!(volts.ph, None, "zero volts is a reading, absence is not");
    assert_close(volts.tds.expect("a TDS voltage"), 0.91);
}

#[tokio::test]
async fn republishing_does_not_restamp_the_probe_voltages() {
    let sink = RecordingSink::default();
    let subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::ph_at(1.5),
        FakeCalibration::standard(),
    );
    let mut subject = subject;

    subject.sample().await;
    subject.republish();

    assert_eq!(sink.count(), 2, "republish does emit a snapshot");
    assert_eq!(
        sink.volts_count(),
        1,
        "a lighting change is not a new probe reading, so the voltages must keep their timestamp"
    );
}

#[tokio::test]
async fn a_tds_voltage_the_board_cannot_produce_reaches_telemetry_as_no_reading() {
    let sink = RecordingSink::default();

    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(3.3),
        FakeCalibration::standard().with(|cal| cal.tds_factor = 0.1),
    )
    .sample()
    .await;

    assert_eq!(sink.last().tds.ppm, None);
    assert_eq!(sink.last().tds.ec_us_cm, None);
    assert_close(sink.last_volts().tds.expect("the raw voltage"), 3.3);
}

#[tokio::test]
async fn a_failed_ph_conversion_blanks_only_ph_and_lends_no_voltage_to_tds() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog {
            ph_volts: Some(1.52),
            tds_volts: Some(0.91),
        },
        FakeCalibration::standard(),
    );

    subject.sample().await;
    subject.sources.analog.ph_volts = None;
    subject.sources.analog.tds_volts = Some(1.2);
    subject.sample().await;

    let volts = sink.last_volts();
    let snapshot = sink.last();
    let expected = tds::convert(1.2, 25.0, 1.0).expect("valid");

    assert_eq!(volts.ph, None);
    assert_close(volts.tds.expect("a TDS voltage"), 1.2);
    assert_eq!(snapshot.co2.current_ph, None);
    assert_close(snapshot.tds.ppm.expect("a reading"), expected.ppm.as_ppm());
}

#[tokio::test]
async fn both_analog_channels_recover_after_the_bus_drops_out() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog {
            ph_volts: Some(1.52),
            tds_volts: Some(0.91),
        },
        FakeCalibration::standard(),
    );

    subject.sample().await;
    subject.sources.analog = FakeAnalog::nothing_attached();
    subject.sample().await;

    let dropped = sink.last_volts();
    assert_eq!((dropped.ph, dropped.tds), (None, None));
    assert_eq!(sink.last().co2.current_ph, None);
    assert_eq!(sink.last().tds.ppm, None);

    subject.sources.analog = FakeAnalog {
        ph_volts: Some(1.5),
        tds_volts: Some(1.1),
    };
    subject.sample().await;

    let recovered = sink.last_volts();
    assert_close(recovered.ph.expect("pH is back"), 1.5);
    assert_close(recovered.tds.expect("TDS is back"), 1.1);
    assert!(sink.last().co2.current_ph.is_some());
    assert!(sink.last().tds.ppm.is_some());
}

#[tokio::test]
async fn the_heater_status_is_copied_into_the_snapshot() {
    let sink = RecordingSink::default();
    let heaters = HeatersStatus {
        tank: HeaterStatus {
            active: true,
            mode: HeaterMode::ManualOn,
            fault: None,
            override_expires_in_s: Some(300),
            target_celsius: 27.0,
        },
        sump: HeaterStatus::unavailable(26.0),
    };

    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            overrides: FakeOverrides::none(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(heaters),
            pumps: FakePumps(PumpsTelemetry::UNAVAILABLE, None),
        },
        &sink,
        config(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().heaters, heaters);
}

#[tokio::test]
async fn the_pump_state_is_copied_into_the_snapshot() {
    let sink = RecordingSink::default();
    let pumps = PumpsTelemetry {
        return_pump: true,
        uv: false,
        makeup: true,
        drain: false,
        operation_kind: Some(OperationKind::TopOff),
        operation_state: OperationState::Refilling,
        circulation_starved: true,
        level_sensors_suspect: true,
        return_held_after_drain: true,
        invalidation_pending: true,
        supervision: Supervision::Unavailable,
    };

    TelemetryCollector::new(
        TelemetrySources {
            temperature: FakeReader::healthy(),
            clock: FakeClock::synced_at_noon_in_june(),
            health: FakeHealth,
            overrides: FakeOverrides::none(),
            levels: FakeLevels::all_dry(),
            analog: FakeAnalog::nothing_attached(),
            calibration: FakeCalibration::standard(),
            heaters: FakeHeaters(HeatersStatus::UNAVAILABLE),
            pumps: FakePumps(pumps, None),
        },
        &sink,
        config(),
    )
    .sample()
    .await;

    assert_eq!(sink.last().pumps, pumps);
}

#[tokio::test(start_paused = true)]
async fn a_sample_publishes_the_tds_reading_with_its_measurement_time() {
    let sink = RecordingSink::default();
    let mut subject = analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::tds_at(2.16),
        FakeCalibration::standard(),
    );
    let before = Instant::now();

    subject.sample().await;
    tokio::time::advance(Duration::from_secs(5)).await;
    subject.republish();

    let samples = sink.tds.lock().unwrap().clone();
    assert_eq!(
        samples.len(),
        1,
        "a republish must not refresh the TDS sample"
    );
    let expected = tds::convert(2.16, 25.0, 1.0).expect("valid").ppm.as_ppm();
    assert_close(samples[0].ppm.expect("a reading"), expected);
    assert!(samples[0].measured_at >= before);
    assert!(samples[0].measured_at <= before + Duration::from_secs(1));
}

#[tokio::test]
async fn a_missing_tds_probe_publishes_an_empty_sample() {
    let sink = RecordingSink::default();
    analog_subject(
        &sink,
        FakeReader::healthy(),
        FakeAnalog::nothing_attached(),
        FakeCalibration::standard(),
    )
    .sample()
    .await;

    assert_eq!(sink.tds.lock().unwrap()[0].ppm, None);
}
