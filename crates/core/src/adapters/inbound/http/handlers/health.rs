use std::sync::Arc;

use picoserve::extract::State;
use picoserve::response::Json;

use crate::domain::metrics::SystemHealthDto;
use crate::ports::inbound::TelemetryQuery;

pub async fn probe<Q: TelemetryQuery>(State(telemetry): State<Arc<Q>>) -> Json<SystemHealthDto> {
    Json(telemetry.snapshot().system_health)
}
