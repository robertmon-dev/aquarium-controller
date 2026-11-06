use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::{FromRef, State};
use picoserve::routing::{get, PathRouter};
use picoserve::Router;

use crate::adapters::inbound::http::auth::ApiTokenLayer;
use crate::adapters::inbound::http::handlers;
use crate::ports::inbound::{LightingControl, TelemetryQuery};

pub struct AppState<Q, L> {
    pub telemetry: Arc<Q>,
    pub lighting: Arc<L>,
}

pub struct LightingState<L>(pub Arc<L>);

impl<Q, L> FromRef<AppState<Q, L>> for Arc<Q> {
    fn from_ref(input: &AppState<Q, L>) -> Self {
        Arc::clone(&input.telemetry)
    }
}

impl<Q, L> FromRef<AppState<Q, L>> for LightingState<L> {
    fn from_ref(input: &AppState<Q, L>) -> Self {
        LightingState(Arc::clone(&input.lighting))
    }
}

pub fn router<Q, L>(
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    token: &'static str,
    sse_keepalive: Duration,
) -> Router<impl PathRouter>
where
    Q: TelemetryQuery + 'static,
    L: LightingControl + 'static,
{
    Router::new()
        .route("/api/telemetry", get(handlers::telemetry::snapshot::<Q>))
        .route(
            "/api/events",
            get(move |state: State<Arc<Q>>| async move {
                handlers::events::stream(state, sse_keepalive).await
            }),
        )
        .route("/api/health", get(handlers::health::probe::<Q>))
        .route(
            "/api/lighting/override",
            get(handlers::lighting::current::<L>)
                .put(handlers::lighting::set::<L>)
                .delete(handlers::lighting::clear::<L>),
        )
        .layer(ApiTokenLayer::new(token))
        .with_state(AppState {
            telemetry,
            lighting,
        })
}
