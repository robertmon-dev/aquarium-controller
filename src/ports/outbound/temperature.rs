use crate::domain::metrics::WaterTemperature;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemperatureSensorRole {
    MainTank,
    Sump,
}

#[derive(Debug, thiserror::Error)]
pub enum TemperatureError {
    #[error("1-Wire bus communication error: {0}")]
    BusError(&'static str),

    #[error("Sensor for role {0:?} was not found on the bus")]
    SensorNotFound(TemperatureSensorRole),

    #[error("CRC mismatch during read from sensor {0:?}")]
    CrcMismatch(TemperatureSensorRole),

    #[error("Temperature reading out of physical bounds: {0}")]
    InvalidReading(&'static str),

    #[error("Sensor reported power-on reset state or disconnected (85.0°C default)")]
    PowerOnResetValue,
}

pub trait TemperatureReader {
    async fn trigger_conversion(&mut self) -> Result<(), TemperatureError>;

    async fn read_temperature(
        &mut self,
        role: TemperatureSensorRole,
    ) -> Result<WaterTemperature, TemperatureError>;

    async fn read_all(
        &mut self,
    ) -> Result<[(TemperatureSensorRole, WaterTemperature); 2], TemperatureError> {
        self.trigger_conversion().await?;

        let tank = self
            .read_temperature(TemperatureSensorRole::MainTank)
            .await?;
        let sump = self.read_temperature(TemperatureSensorRole::Sump).await?;

        Ok([
            (TemperatureSensorRole::MainTank, tank),
            (TemperatureSensorRole::Sump, sump),
        ])
    }
}
