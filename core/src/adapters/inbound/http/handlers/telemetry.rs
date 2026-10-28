use std::sync::Arc;

use picoserve::extract::State;
use picoserve::response::Json;

use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::TelemetryQuery;

pub async fn snapshot<Q: TelemetryQuery>(
    State(telemetry): State<Arc<Q>>,
) -> Json<TankTelemetryDto> {
    tracing::info!("GET /api/telemetry");
    Json(telemetry.snapshot())
}
