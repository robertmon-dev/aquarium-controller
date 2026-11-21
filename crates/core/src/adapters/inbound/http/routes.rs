use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::{FromRef, State};
use picoserve::routing::{get, parse_path_segment, post, put, PathRouter};
use picoserve::Router;

use crate::adapters::inbound::http::api::ApiLayer;
use crate::adapters::inbound::http::handlers;
use crate::domain::heater::HeaterId;
use crate::domain::pump::PumpId;
use crate::ports::inbound::{
    CalibrationControl, HeaterControl, LightingControl, ProbeVoltageQuery, PumpControl,
    TelemetryQuery,
};

pub struct AppState<Q, L, C, H, P> {
    pub telemetry: Arc<Q>,
    pub lighting: Arc<L>,
    pub calibration: Arc<C>,
    pub heaters: Arc<H>,
    pub pumps: Arc<P>,
}

pub struct LightingState<L>(pub Arc<L>);

pub struct CalibrationState<C>(pub Arc<C>);

pub struct HeaterState<H>(pub Arc<H>);

pub struct PumpState<P>(pub Arc<P>);

impl<Q, L, C, H, P> FromRef<AppState<Q, L, C, H, P>> for Arc<Q> {
    fn from_ref(input: &AppState<Q, L, C, H, P>) -> Self {
        Arc::clone(&input.telemetry)
    }
}

impl<Q, L, C, H, P> FromRef<AppState<Q, L, C, H, P>> for LightingState<L> {
    fn from_ref(input: &AppState<Q, L, C, H, P>) -> Self {
        LightingState(Arc::clone(&input.lighting))
    }
}

impl<Q, L, C, H, P> FromRef<AppState<Q, L, C, H, P>> for HeaterState<H> {
    fn from_ref(input: &AppState<Q, L, C, H, P>) -> Self {
        HeaterState(Arc::clone(&input.heaters))
    }
}

impl<Q, L, C, H, P> FromRef<AppState<Q, L, C, H, P>> for PumpState<P> {
    fn from_ref(input: &AppState<Q, L, C, H, P>) -> Self {
        PumpState(Arc::clone(&input.pumps))
    }
}

impl<Q, L, C, H, P> FromRef<AppState<Q, L, C, H, P>> for CalibrationState<C> {
    fn from_ref(input: &AppState<Q, L, C, H, P>) -> Self {
        CalibrationState(Arc::clone(&input.calibration))
    }
}

#[allow(clippy::too_many_arguments)]
pub fn router<Q, L, C, H, P>(
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    calibration: Arc<C>,
    heaters: Arc<H>,
    pumps: Arc<P>,
    token: &'static str,
    sse_keepalive: Duration,
    cors_origin: &'static str,
) -> Router<impl PathRouter>
where
    Q: TelemetryQuery + ProbeVoltageQuery + 'static,
    L: LightingControl + 'static,
    C: CalibrationControl + 'static,
    H: HeaterControl + 'static,
    P: PumpControl + 'static,
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
        .route("/api/heaters", get(handlers::heaters::status::<H>))
        .route(
            "/api/heaters/settings",
            get(handlers::heaters::settings::<H>).patch(handlers::heaters::patch_settings::<H>),
        )
        .route(
            (
                "/api/heaters",
                parse_path_segment::<HeaterId>(),
                "/override",
            ),
            put(handlers::heaters::set_override::<H>)
                .delete(handlers::heaters::clear_override::<H>),
        )
        .route("/api/pumps", get(handlers::pumps::status::<P>))
        .route(
            "/api/pumps/settings",
            get(handlers::pumps::settings::<P>).patch(handlers::pumps::patch_settings::<P>),
        )
        .route(
            ("/api/pumps", parse_path_segment::<PumpId>(), "/override"),
            put(handlers::pumps::set_override::<P>).delete(handlers::pumps::clear_override::<P>),
        )
        .route(
            (
                "/api/pumps",
                parse_path_segment::<PumpId>(),
                "/runtime/reset",
            ),
            post(handlers::pumps::reset_runtime::<P>),
        )
        .route(
            "/api/pumps/top-off",
            post(handlers::pumps::start_top_off::<P>).delete(handlers::pumps::cancel_top_off::<P>),
        )
        .route(
            "/api/pumps/top-off/resume",
            post(handlers::pumps::resume_top_off::<P>),
        )
        .route(
            "/api/water-change",
            post(handlers::pumps::start_water_change::<P>)
                .delete(handlers::pumps::cancel_water_change::<P>),
        )
        .route(
            "/api/water-change/resume",
            post(handlers::pumps::resume_water_change::<P>),
        )
        .layer(ApiLayer::new(token, cors_origin))
        .with_state(AppState {
            telemetry,
            lighting,
            calibration,
            heaters,
            pumps,
        })
}
