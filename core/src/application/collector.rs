use crate::domain::light::{self, LightingSchedule};
use crate::domain::metrics::{LightingMetricsDto, LightingProfile, TankTelemetryDto};
use crate::ports::outbound::{
    Clock, LightingOverrideSource, SystemHealthProvider, TelemetrySink, TemperatureReader,
    TemperatureSensorRole,
};

pub struct TelemetryCollector<R, C, H, S, O> {
    reader: R,
    clock: C,
    health: H,
    sink: S,
    overrides: O,
    lighting: LightingSchedule,
    target_celsius: f32,
    last_readings: (Option<f32>, Option<f32>),
}

impl<R, C, H, S, O> TelemetryCollector<R, C, H, S, O>
where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink,
    O: LightingOverrideSource,
{
    pub fn new(reader: R, clock: C, health: H, sink: S, overrides: O, target_celsius: f32) -> Self {
        Self {
            reader,
            clock,
            health,
            sink,
            overrides,
            lighting: LightingSchedule::new(),
            target_celsius,
            last_readings: (None, None),
        }
    }

    pub async fn sample(&mut self) {
        self.last_readings = self.read_sensors().await;
        self.publish();
    }

    pub fn republish(&self) {
        self.publish();
    }

    fn publish(&self) {
        let (tank, sump) = self.last_readings;
        self.sink.publish(self.compose(tank, sump));
    }

    async fn read_sensors(&mut self) -> (Option<f32>, Option<f32>) {
        if self.reader.trigger_conversion().await.is_err() {
            return (None, None);
        }

        let tank = self
            .reader
            .read_temperature(TemperatureSensorRole::MainTank)
            .await
            .ok()
            .map(|t| t.as_celsius());

        let sump = self
            .reader
            .read_temperature(TemperatureSensorRole::Sump)
            .await
            .ok()
            .map(|t| t.as_celsius());

        (tank, sump)
    }

    fn compose(&self, tank: Option<f32>, sump: Option<f32>) -> TankTelemetryDto {
        let mut dto = TankTelemetryDto::empty();

        dto.timestamp_epoch_s = self.clock.epoch_seconds().unwrap_or(0);

        dto.system_health.uptime_seconds = self.clock.uptime_seconds();
        dto.system_health.wifi_rssi_dbm = self.health.wifi_rssi_dbm();
        dto.system_health.free_heap_kb = self.health.free_heap_kb();

        dto.temperature.tank_celsius = tank;
        dto.temperature.sump_celsius = sump;
        dto.temperature.target_celsius = self.target_celsius;
        dto.temperature.sensors_healthy = tank.is_some() && sump.is_some();

        dto.lighting = self.evaluate_lighting();

        dto
    }

    fn evaluate_lighting(&self) -> LightingMetricsDto {
        let scheduled = match (self.clock.local_minute_of_day(), self.clock.local_month()) {
            (Some(minute), Some(month)) => self.lighting.evaluate(minute, month),
            _ => LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            },
        };

        light::resolve(scheduled, self.overrides.active_override().as_ref())
    }
}

#[cfg(test)]
mod tests;
