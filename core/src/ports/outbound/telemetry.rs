use crate::domain::metrics::TankTelemetryDto;

pub trait TelemetrySink {
    fn publish(&self, snapshot: TankTelemetryDto);
}
