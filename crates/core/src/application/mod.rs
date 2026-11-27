pub mod calibration;
pub mod collector;
pub mod heater;
pub mod lighting;
pub mod metrics_export;
pub mod pump;
pub mod reservoir;
pub mod sump_light;
pub mod water_level;

pub use calibration::CalibrationService;
pub use collector::{TelemetryCollector, TelemetryConfig, TelemetrySources};
pub use heater::{heater_loop, HeaterController, HeaterService};
pub use lighting::{lighting_loop, LightingService};
pub use metrics_export::{export_loop, MetricsExporter};
pub use pump::{pump_loop, PumpController, PumpService, PumpTiming};
pub use reservoir::{reservoir_loop, ReservoirController, ReservoirService, ReservoirTiming};
pub use sump_light::{sump_light_loop, SumpLightService};
pub use water_level::WaterLevelSampler;

async fn sleep_until_some(at: Option<tokio::time::Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => core::future::pending().await,
    }
}
