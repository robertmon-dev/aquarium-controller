use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::{FromRef, State};
use picoserve::routing::{get, parse_path_segment, PathRouter};
use picoserve::Router;

use crate::adapters::inbound::http::api::ApiLayer;
use crate::adapters::inbound::http::handlers;
use crate::ports::inbound::{
    CalibrationControl, LightingControl, ProbeVoltageQuery, TelemetryQuery,
};

pub struct AppState<Q, L, C> {
    pub telemetry: Arc<Q>,
    pub lighting: Arc<L>,
    pub calibration: Arc<C>,
}

pub struct LightingState<L>(pub Arc<L>);

pub struct CalibrationState<C>(pub Arc<C>);

impl<Q, L, C> FromRef<AppState<Q, L, C>> for Arc<Q> {
    fn from_ref(input: &AppState<Q, L, C>) -> Self {
        Arc::clone(&input.telemetry)
    }
}

impl<Q, L, C> FromRef<AppState<Q, L, C>> for LightingState<L> {
    fn from_ref(input: &AppState<Q, L, C>) -> Self {
        LightingState(Arc::clone(&input.lighting))
    }
}

impl<Q, L, C> FromRef<AppState<Q, L, C>> for CalibrationState<C> {
    fn from_ref(input: &AppState<Q, L, C>) -> Self {
        CalibrationState(Arc::clone(&input.calibration))
    }
}

pub fn router<Q, L, C>(
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    calibration: Arc<C>,
    token: &'static str,
    sse_keepalive: Duration,
    cors_origin: &'static str,
) -> Router<impl PathRouter>
where
    Q: TelemetryQuery + ProbeVoltageQuery + 'static,
    L: LightingControl + 'static,
    C: CalibrationControl + 'static,
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
        .route("/api/probe", get(handlers::probe::voltages::<Q>))
        .route(
            "/api/lighting/override",
            get(handlers::lighting::current::<L>)
                .put(handlers::lighting::set::<L>)
                .delete(handlers::lighting::clear::<L>),
        )
        .route(
            ("/api/lighting/schedule", parse_path_segment::<u8>()),
            get(handlers::lighting::schedule),
        )
        .route(
            "/api/calibration",
            get(handlers::calibration::current::<C>).patch(handlers::calibration::patch::<C>),
        )
        .layer(ApiLayer::new(token, cors_origin))
        .with_state(AppState {
            telemetry,
            lighting,
            calibration,
        })
}
