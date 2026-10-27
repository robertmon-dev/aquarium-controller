use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::State;
use picoserve::routing::{get, PathRouter};
use picoserve::Router;

use crate::adapters::inbound::http::auth::ApiTokenLayer;
use crate::adapters::inbound::http::handlers;
use crate::ports::inbound::TelemetryQuery;

pub fn router<Q: TelemetryQuery + 'static>(
    telemetry: Arc<Q>,
    token: &'static str,
    sse_keepalive: Duration,
) -> Router<impl PathRouter> {
    Router::new()
        .route("/api/telemetry", get(handlers::telemetry::snapshot::<Q>))
        .route(
            "/api/events",
            get(move |state: State<Arc<Q>>| async move {
                handlers::events::stream(state, sse_keepalive).await
            }),
        )
        .route("/api/health", get(handlers::health::probe::<Q>))
        .layer(ApiTokenLayer::new(token))
        .with_state(telemetry)
}
