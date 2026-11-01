#[cfg(target_os = "espidf")]
pub use hw::TemperatureSource;

#[cfg(target_os = "espidf")]
mod hw {
    use aquarium_core::domain::metrics::WaterTemperature;
    use aquarium_core::ports::outbound::temperature::{
        TemperatureError, TemperatureReader, TemperatureSensorRole,
    };

    use crate::adapters::outbound::ds18b20::Ds18b20Adapter;

    pub enum TemperatureSource<'d> {
        Ds18b20(Ds18b20Adapter<'d>),
        Disabled,
    }

    impl TemperatureReader for TemperatureSource<'_> {
        async fn trigger_conversion(&mut self) -> Result<(), TemperatureError> {
            match self {
                Self::Ds18b20(adapter) => adapter.trigger_conversion().await,
                Self::Disabled => Err(TemperatureError::BusError(
                    "temperature component is disabled in settings",
                )),
            }
        }

        async fn read_temperature(
            &mut self,
            role: TemperatureSensorRole,
        ) -> Result<WaterTemperature, TemperatureError> {
            match self {
                Self::Ds18b20(adapter) => adapter.read_temperature(role).await,
                Self::Disabled => Err(TemperatureError::SensorNotFound(role)),
            }
        }
    }
}
