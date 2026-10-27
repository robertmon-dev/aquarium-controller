pub mod clock;
pub mod system_health;
pub mod telemetry;
pub mod temperature;

pub use clock::Clock;
pub use system_health::SystemHealthProvider;
pub use telemetry::TelemetrySink;
pub use temperature::{TemperatureError, TemperatureReader, TemperatureSensorRole};
