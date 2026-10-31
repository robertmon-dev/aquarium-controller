use std::sync::Mutex;

use super::TelemetryCollector;
use crate::domain::light::{ChannelLevels, LightingOverride, LightingTarget};
use crate::domain::metrics::{LightingProfile, TankTelemetryDto, WaterTemperature};
use crate::ports::outbound::{
    Clock, LightingOverrideSource, SystemHealthProvider, TelemetrySink, TemperatureError,
    TemperatureReader, TemperatureSensorRole,
};

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

fn collector(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
) -> TelemetryCollector<FakeReader, FakeClock, FakeHealth, &RecordingSink, FakeOverrides> {
    with_overrides(reader, clock, sink, FakeOverrides::none())
}

fn with_overrides(
    reader: FakeReader,
    clock: FakeClock,
    sink: &RecordingSink,
    overrides: FakeOverrides,
) -> TelemetryCollector<FakeReader, FakeClock, FakeHealth, &RecordingSink, FakeOverrides> {
    TelemetryCollector::new(reader, clock, FakeHealth, sink, overrides, 27.0)
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
    // A child module can read private fields of its ancestor, so the test
    // reaches the counter directly instead of widening the public surface.
    let after_sample = c.reader.conversions;

    c.republish();
    c.republish();

    assert_eq!(sink.count(), 3, "every call publishes a snapshot");
    assert_eq!(
        c.reader.conversions, after_sample,
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
