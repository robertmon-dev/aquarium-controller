use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::State;
use picoserve::response::{EventStream, IntoResponse};

use crate::adapters::inbound::http::sse::TelemetryEventSource;
use crate::ports::inbound::TelemetryQuery;

pub async fn stream<Q: TelemetryQuery>(
    State(telemetry): State<Arc<Q>>,
    keepalive: Duration,
) -> impl IntoResponse {
    EventStream(TelemetryEventSource::new(telemetry.subscribe(), keepalive)).into_response()
}
