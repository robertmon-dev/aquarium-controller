use std::time::Duration;

use tokio::time::Instant;

use crate::domain::calibration::{Calibration, PhCalibration};
use crate::domain::metrics::{TankTelemetryDto, WaterLevelMetricsDto};
use crate::domain::pump::{LevelConditions, OperationKind};
use crate::ports::outbound::{
    CalibrationSource, Clock, HeaterStatusSource, LightingStatusSource, PhReader, ProbeVoltageSink,
    PumpStatusSource, ReservoirStatusSource, SystemHealthProvider, TdsReader, TdsSample,
    TdsSampleSink, TelemetrySink, TemperatureReader, TemperatureSample, TemperatureSampleSink,
    TemperatureSensorRole, WaterLevelSource,
};
use crate::protocol::{ph, tds};

pub struct TelemetrySources<R, C, H, O, W, A, K, T, P, V> {
    pub temperature: R,
    pub clock: C,
    pub health: H,
    pub lighting: O,
    pub levels: W,
    pub analog: A,
    pub calibration: K,
    pub heaters: T,
    pub pumps: P,
    pub reservoir: V,
}

#[derive(Debug, Clone, Copy)]
pub struct TelemetryConfig {
    pub target_celsius: f32,
    pub ph_nominal_volts_per_ph: f32,
    pub level_stale: Duration,
}

#[derive(Debug, Clone, Copy, Default)]
struct Readings {
    tank: Option<f32>,
    sump: Option<f32>,
    ph_volts: Option<f32>,
    tds_volts: Option<f32>,
}

pub struct TelemetryCollector<R, C, H, S, O, W, A, K, T, P, V> {
    sources: TelemetrySources<R, C, H, O, W, A, K, T, P, V>,
    sink: S,
    config: TelemetryConfig,
    last: Readings,
}

impl<R, C, H, S, O, W, A, K, T, P, V> TelemetryCollector<R, C, H, S, O, W, A, K, T, P, V>
where
    R: TemperatureReader,
    C: Clock,
    H: SystemHealthProvider,
    S: TelemetrySink + ProbeVoltageSink + TemperatureSampleSink + TdsSampleSink,
    O: LightingStatusSource,
    W: WaterLevelSource,
    A: PhReader + TdsReader,
    K: CalibrationSource,
    T: HeaterStatusSource,
    P: PumpStatusSource,
    V: ReservoirStatusSource,
{
    pub fn new(
        sources: TelemetrySources<R, C, H, O, W, A, K, T, P, V>,
        sink: S,
        config: TelemetryConfig,
    ) -> Self {
        Self {
            sources,
            sink,
            config,
            last: Readings::default(),
        }
    }

    pub async fn sample(&mut self) {
        let measured_at = Instant::now();
        self.last = self.read_sensors().await;

        self.sink.publish_temperature(TemperatureSample {
            tank_celsius: self.last.tank,
            sump_celsius: self.last.sump,
            measured_at,
        });

        let calibration = self.sources.calibration.calibration();
        self.sink.publish_tds(TdsSample {
            ppm: self
                .resolve_tds(self.last.tds_volts, &calibration)
                .map(|reading| reading.ppm.as_ppm()),
            measured_at,
        });

        let snapshot = self.compose(self.last);

        self.sink.publish_volts(
            self.last.ph_volts,
            self.last.tds_volts,
            snapshot.timestamp_epoch_s,
        );
        self.sink.publish(snapshot);
    }

    pub fn republish(&self) {
        self.publish();
    }

    fn publish(&self) {
        self.sink.publish(self.compose(self.last));
    }

    async fn read_sensors(&mut self) -> Readings {
        let ph_volts = PhReader::read_volts(&mut self.sources.analog).await.ok();
        let tds_volts = TdsReader::read_volts(&mut self.sources.analog).await.ok();

        if self.sources.temperature.trigger_conversion().await.is_err() {
            return Readings {
                tank: None,
                sump: None,
                ph_volts,
                tds_volts,
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
            tds_volts,
        }
    }

    fn compose(&self, readings: Readings) -> TankTelemetryDto {
        let Readings {
            tank,
            sump,
            ph_volts,
            tds_volts,
        } = readings;
        let calibration = self.sources.calibration.calibration();
        let mut dto = TankTelemetryDto::empty();

        dto.timestamp_epoch_s = self.sources.clock.epoch_seconds().unwrap_or(0);

        dto.system_health.uptime_seconds = self.sources.clock.uptime_seconds();
        dto.system_health.wifi_rssi_dbm = self.sources.health.wifi_rssi_dbm();
        dto.system_health.free_heap_kb = self.sources.health.free_heap_kb();

        dto.temperature.tank_celsius = tank;
        dto.temperature.sump_celsius = sump;
        dto.temperature.sensors_healthy = tank.is_some() && sump.is_some();

        dto.co2.current_ph = self.resolve_ph(ph_volts, sump, &calibration.ph);
        dto.co2.ph_electrode_health = self.electrode_health(&calibration.ph);

        if let Some(reading) = self.resolve_tds(tds_volts, &calibration) {
            dto.tds.ppm = Some(reading.ppm.as_ppm());
            dto.tds.ec_us_cm = Some(reading.ec_us_cm);
        }

        dto.lighting = self.sources.lighting.lighting_status();
        let now = Instant::now();
        let levels = self.sources.levels.levels().filter(|sample| {
            now.saturating_duration_since(sample.measured_at) < self.config.level_stale
        });
        if let Some(sample) = levels {
            dto.water_level = WaterLevelMetricsDto::from(&sample.levels);
            dto.ato.sump_water_low = sample.levels.sump_water_low();
            dto.ato.reservoir_empty =
                LevelConditions::from_snapshot(&sample.levels, None).reservoir_empty;
        }
        dto.heaters = self.sources.heaters.heater_status();
        dto.pumps = self.sources.pumps.pump_telemetry();
        dto.flood = self.sources.pumps.flood_status();
        dto.leak_detected = dto.flood.probes.is_some_and(|probes| probes.wet());
        if dto.flood.latched {
            dto.system_mode = crate::domain::metrics::SystemMode::EmergencyLockout;
        }
        dto.reservoir = self.sources.reservoir.reservoir_telemetry();
        dto.ato.pump_active =
            dto.pumps.operation_kind == Some(OperationKind::TopOff) && dto.pumps.makeup;
        dto.ato.last_fill_duration_ms = self
            .sources
            .pumps
            .last_top_off()
            .map_or(0, |record| record.refill_ran_s.saturating_mul(1000));

        dto
    }

    fn resolve_ph(
        &self,
        volts: Option<f32>,
        sump_celsius: Option<f32>,
        cal: &PhCalibration,
    ) -> Option<f32> {
        let volts = volts?;
        let celsius = sump_celsius.unwrap_or(self.config.target_celsius);

        ph::convert(volts, celsius, cal)
            .ok()
            .map(|reading| reading.value())
    }

    fn resolve_tds(
        &self,
        volts: Option<f32>,
        calibration: &Calibration,
    ) -> Option<tds::TdsReading> {
        tds::convert(
            volts?,
            calibration.makeup_tank_celsius,
            calibration.tds_factor,
        )
        .ok()
    }

    fn electrode_health(&self, cal: &PhCalibration) -> Option<f32> {
        if cal.validate().is_err() || self.config.ph_nominal_volts_per_ph <= 0.0 {
            return None;
        }

        Some(cal.volts_per_ph() / self.config.ph_nominal_volts_per_ph)
    }
}

#[cfg(test)]
mod tests;
