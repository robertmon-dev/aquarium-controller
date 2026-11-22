use tokio::time::Instant;

use crate::domain::metrics::TankTelemetryDto;

pub trait TelemetrySink {
    fn publish(&self, snapshot: TankTelemetryDto);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TemperatureSample {
    pub tank_celsius: Option<f32>,
    pub sump_celsius: Option<f32>,
    pub measured_at: Instant,
}

pub trait TemperatureSampleSink {
    fn publish_temperature(&self, sample: TemperatureSample);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TdsSample {
    pub ppm: Option<f32>,
    pub measured_at: Instant,
}

pub trait TdsSampleSink {
    fn publish_tds(&self, sample: TdsSample);
}
