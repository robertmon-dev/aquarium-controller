use std::sync::Mutex;

use super::{TelemetryCollector, TelemetryConfig, TelemetrySources};
use crate::domain::light::{ChannelLevels, LightingOverride, LightingTarget};
use crate::domain::metrics::{LightingProfile, TankTelemetryDto, WaterTemperature};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::outbound::{
    Clock, LightingOverrideSource, PhReader, SystemHealthProvider, TelemetrySink, TemperatureError,
    TemperatureReader, TemperatureSensorRole, WaterLevelSource,
};
use crate::protocol::ph::{self, PhCalibration, PhError};

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

struct FakeLevels(WaterLevelSnapshot);

impl FakeLevels {
    fn all_dry() -> Self {
        Self(WaterLevelSnapshot::dry())
    }

    fn wet(roles: &[WaterLevelRole]) -> Self {
        let mut snapshot = WaterLevelSnapshot::dry();
        for role in roles {
            snapshot.sensors[role.index()].state = WaterLevelState::Wet;
        }
        Self(snapshot)
    }

    fn faulty(role: WaterLevelRole) -> Self {
        let mut snapshot = WaterLevelSnapshot::dry();
        snapshot.sensors[role.index()].faulty = true;
        Self(snapshot)
    }
}

impl WaterLevelSource for FakeLevels {
    fn levels(&self) -> WaterLevelSnapshot {
        self.0.clone()
    }
}

struct FakePh(Option<f32>);

impl FakePh {
    fn at(volts: f32) -> Self {
        Self(Some(volts))
    }

    fn unavailable() -> Self {
        Self(None)
    }
}

impl PhReader for FakePh {
    async fn read_volts(&mut self) -> Result<f32, PhError> {
        self.0
            .ok_or(PhError::Unavailable("fake: no probe attached"))
    }
}

const CAL: PhCalibration = PhCalibration {
    neutral_volts: 1.500,
    acid_volts: 1.67748,
    calibrated_at_celsius: 25.0,
};

const NOMINAL_VOLTS_PER_PH: f32 = 0.05916;

fn config() -> TelemetryConfig {
    TelemetryConfig {
        target_celsius: 27.0,
        ph: CAL,
        ph_nominal_volts_per_ph: NOMINAL_VOLTS_PER_PH,
    }
}

struct FakeHealth;

impl SystemHealthProvider for FakeHealth {
    fn wifi_rssi_dbm(&self) -> i8 {
        -57
    }
    fn free_heap_kb(&self) -> u32 {
        180
    }
}

#[derive(Default)]
struct RecordingSink(Mutex<Vec<TankTelemetryDto>>);

impl RecordingSink {
    fn last(&self) -> TankTelemetryDto {
        self.0
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("the sink received nothing")
    }

    fn count(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

impl TelemetrySink for &RecordingSink {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.0.lock().unwrap().push(snapshot);
    }
}

type Subject<'a> = TelemetryCollector<
    FakeReader,
    FakeClock,
    FakeHealth,
    &'a RecordingSink,
    FakeOverrides,
    FakeLevels,
    FakePh,
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
    with_ph(
        reader,
        clock,
        sink,
        overrides,
        levels,
        FakePh::unavailable(),
    )
}

fn with_ph(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    overrides: FakeOverrides,
    levels: FakeLevels,
    ph: FakePh,
) -> Subject<'_> {
    TelemetryCollector::new(
        TelemetrySources {
            temperature: reader,
            clock,
            health: FakeHealth,
            overrides,
            levels,
            ph,
        },
        sink,
        config(),
    )
}

fn ph_subject(sink: &RecordingSink, reader: FakeReader, ph: FakePh) -> Subject<'_> {
    with_ph(
        reader,
        FakeClock::synced_at_noon_in_june(),
        sink,
        FakeOverrides::none(),
        FakeLevels::all_dry(),
        ph,
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
    assert_eq!(s.temperature.target_celsius, 27.0);
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
    assert_eq!(s.system_health.wifi_rssi_dbm, -57);
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
async fn the_ato_section_stays_untouched_by_the_level_driver() {
    let sink = RecordingSink::default();
    with_levels(&sink, FakeLevels::wet(&[WaterLevelRole::SumpPump]))
        .sample()
        .await;

    let s = sink.last();
    assert!(
        !s.ato.sump_water_low,
        "interpreting levels into ATO state belongs to the ATO spec, not here"
    );
    assert!(!s.ato.reservoir_empty);
    assert!(!s.ato.pump_active);
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

    ph_subject(&sink, reader, FakePh::at(CAL.acid_volts))
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

    ph_subject(&sink, FakeReader::healthy(), FakePh::unavailable())
        .sample()
        .await;

    assert_eq!(sink.last().co2.current_ph, None);
}

#[tokio::test]
async fn compensation_uses_the_tank_temperature_not_the_sump() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: Some(25.0),
        sump: Some(60.0),
        trigger_fails: false,
        conversions: 0,
    };

    ph_subject(&sink, reader, FakePh::at(CAL.acid_volts))
        .sample()
        .await;

    let from_tank = ph::convert(CAL.acid_volts, 25.0, &CAL)
        .expect("valid")
        .value();
    let from_sump = ph::convert(CAL.acid_volts, 60.0, &CAL)
        .expect("valid")
        .value();

    let reported = sink.last().co2.current_ph.expect("a reading");
    assert_close(reported, from_tank);
    assert!(
        (reported - from_sump).abs() > 0.1,
        "the sump temperature must not drive pH compensation"
    );
}

#[tokio::test]
async fn without_a_tank_reading_compensation_falls_back_to_the_target() {
    let sink = RecordingSink::default();
    let reader = FakeReader {
        tank: None,
        sump: Some(26.0),
        trigger_fails: false,
        conversions: 0,
    };

    ph_subject(&sink, reader, FakePh::at(CAL.acid_volts))
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

    ph_subject(&sink, FakeReader::healthy(), FakePh::at(CAL.neutral_volts))
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

    let mut subject = ph_subject(&sink, FakeReader::healthy(), FakePh::at(1.5));
    subject.config.ph = worn;
    subject.sample().await;

    let health = sink.last().co2.ph_electrode_health.expect("a ratio");
    assert!(
        health > 0.8 && health < 0.9,
        "150 mV over three pH units is about 85% of nominal, got {health}"
    );
}

#[tokio::test]
async fn republish_keeps_the_last_ph_reading() {
    let sink = RecordingSink::default();
    let mut subject = ph_subject(&sink, FakeReader::healthy(), FakePh::at(CAL.acid_volts));

    subject.sample().await;
    subject.republish();

    assert!(sink.last().co2.current_ph.is_some());
}
