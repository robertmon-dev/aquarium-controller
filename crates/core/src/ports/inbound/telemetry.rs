use tokio::sync::watch;

use crate::domain::metrics::TankTelemetryDto;

pub trait TelemetryQuery {
    fn snapshot(&self) -> TankTelemetryDto;
    fn subscribe(&self) -> watch::Receiver<TankTelemetryDto>;
}
