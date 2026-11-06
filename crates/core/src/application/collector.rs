use crate::domain::light::{self, LightingSchedule};
use crate::domain::metrics::{
    LightingMetricsDto, LightingProfile, TankTelemetryDto, WaterLevelMetricsDto,
};
use crate::ports::outbound::{
    Clock, LightingOverrideSource, PhReader, SystemHealthProvider, TelemetrySink,
    TemperatureReader, TemperatureSensorRole, WaterLevelSource,
};
use crate::protocol::ph::{self, PhCalibration};

pub struct TelemetrySources<R, C, H, O, W, P> {
    pub temperature: R,
    pub clock: C,
    pub health: H,
    pub overrides: O,
    pub levels: W,
    pub ph: P,
}

#[derive(Debug, Clone, Copy)]
pub struct TelemetryConfig {
    pub target_celsius: f32,
    pub ph: PhCalibration,
    pub ph_nominal_volts_per_ph: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Readings {
    tank: Option<f32>,
    sump: Option<f32>,
    ph_volts: Option<f32>,
}

pub struct TelemetryCollector<R, C, H, S, O, W, P> {
    sources: TelemetrySources<R, C, H, O, W, P>,
    sink: S,
    lighting: LightingSchedule,
    config: TelemetryConfig,
    last: Readings,
}

impl<R, C, H, S, O, W, P> TelemetryCollector<R, C, H, S, O, W, P>
where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink,
    O: LightingOverrideSource,
    W: WaterLevelSource,
    P: PhReader,
{
    pub fn new(
        sources: TelemetrySources<R, C, H, O, W, P>,
        sink: S,
        config: TelemetryConfig,
    ) -> Self {
        Self {
            sources,
            sink,
            lighting: LightingSchedule::new(),
            config,
            last: Readings::default(),
        }
    }

    pub async fn sample(&mut self) {
        self.last = self.read_sensors().await;
        self.publish();
    }

    pub fn republish(&self) {
        self.publish();
    }

    fn publish(&self) {
        self.sink.publish(self.compose(self.last));
    }

    async fn read_sensors(&mut self) -> Readings {
        let ph_volts = self.sources.ph.read_volts().await.ok();

        if self.sources.temperature.trigger_conversion().await.is_err() {
            return Readings {
                tank: None,
                sump: None,
                ph_volts,
            };
        }

        let tank = self
            .sources
            .temperature
            .read_temperature(TemperatureSensorRole::MainTank)
            .await
            .ok()
            .map(|t| t.as_celsius());

        let sump = self
            .sources
            .temperature
            .read_temperature(TemperatureSensorRole::Sump)
            .await
            .ok()
            .map(|t| t.as_celsius());

        Readings {
            tank,
            sump,
            ph_volts,
        }
    }

    fn compose(&self, readings: Readings) -> TankTelemetryDto {
        let Readings {
            tank,
            sump,
            ph_volts,
        } = readings;
        let mut dto = TankTelemetryDto::empty();

        dto.timestamp_epoch_s = self.sources.clock.epoch_seconds().unwrap_or(0);

        dto.system_health.uptime_seconds = self.sources.clock.uptime_seconds();
        dto.system_health.wifi_rssi_dbm = self.sources.health.wifi_rssi_dbm();
        dto.system_health.free_heap_kb = self.sources.health.free_heap_kb();

        dto.temperature.tank_celsius = tank;
        dto.temperature.sump_celsius = sump;
        dto.temperature.target_celsius = self.config.target_celsius;
        dto.temperature.sensors_healthy = tank.is_some() && sump.is_some();

        dto.co2.current_ph = self.resolve_ph(ph_volts, tank);
        dto.co2.ph_electrode_health = self.electrode_health();

        dto.lighting = self.evaluate_lighting();
        let levels = self.sources.levels.levels();
        dto.water_level = WaterLevelMetricsDto::from(&levels);
        dto.ato.sump_water_low = levels.sump_water_low();

        dto
    }

    fn resolve_ph(&self, volts: Option<f32>, tank_celsius: Option<f32>) -> Option<f32> {
        let volts = volts?;
        let celsius = tank_celsius.unwrap_or(self.config.target_celsius);

        ph::convert(volts, celsius, &self.config.ph)
            .ok()
            .map(|reading| reading.value())
    }

    fn electrode_health(&self) -> Option<f32> {
        if self.config.ph.validate().is_err() || self.config.ph_nominal_volts_per_ph <= 0.0 {
            return None;
        }

        Some(self.config.ph.volts_per_ph() / self.config.ph_nominal_volts_per_ph)
    }

    fn evaluate_lighting(&self) -> LightingMetricsDto {
        let scheduled = match (
            self.sources.clock.local_minute_of_day(),
            self.sources.clock.local_month(),
        ) {
            (Some(minute), Some(month)) => self.lighting.evaluate(minute, month),
            _ => LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            },
        };

        light::resolve(scheduled, self.sources.overrides.active_override().as_ref())
    }
}

#[cfg(test)]
mod tests;
