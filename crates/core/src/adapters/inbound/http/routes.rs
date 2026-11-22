use std::sync::Arc;
use std::time::Duration;

use picoserve::extract::{FromRef, State};
use picoserve::routing::{delete, get, parse_path_segment, post, put, PathRouter};
use picoserve::Router;

use crate::adapters::inbound::http::api::ApiLayer;
use crate::adapters::inbound::http::handlers;
use crate::domain::heater::HeaterId;
use crate::domain::pump::PumpId;
use crate::domain::reservoir::{OverrideTarget, ReservoirChannel};
use crate::ports::inbound::{
    CalibrationControl, HeaterControl, LightingControl, ProbeVoltageQuery, PumpControl,
    ReservoirControl, TelemetryQuery,
};

pub struct AppState<Q, L, C, H, P, R> {
    pub telemetry: Arc<Q>,
    pub lighting: Arc<L>,
    pub calibration: Arc<C>,
    pub heaters: Arc<H>,
    pub pumps: Arc<P>,
    pub reservoir: Arc<R>,
}

pub struct LightingState<L>(pub Arc<L>);

pub struct CalibrationState<C>(pub Arc<C>);

pub struct HeaterState<H>(pub Arc<H>);

pub struct PumpState<P>(pub Arc<P>);

pub struct ReservoirState<R>(pub Arc<R>);

impl<Q, L, C, H, P, R> FromRef<AppState<Q, L, C, H, P, R>> for Arc<Q> {
    fn from_ref(input: &AppState<Q, L, C, H, P, R>) -> Self {
        Arc::clone(&input.telemetry)
    }
}

impl<Q, L, C, H, P, R> FromRef<AppState<Q, L, C, H, P, R>> for LightingState<L> {
    fn from_ref(input: &AppState<Q, L, C, H, P, R>) -> Self {
        LightingState(Arc::clone(&input.lighting))
    }
}

impl<Q, L, C, H, P, R> FromRef<AppState<Q, L, C, H, P, R>> for HeaterState<H> {
    fn from_ref(input: &AppState<Q, L, C, H, P, R>) -> Self {
        HeaterState(Arc::clone(&input.heaters))
    }
}

impl<Q, L, C, H, P, R> FromRef<AppState<Q, L, C, H, P, R>> for PumpState<P> {
    fn from_ref(input: &AppState<Q, L, C, H, P, R>) -> Self {
        PumpState(Arc::clone(&input.pumps))
    }
}

impl<Q, L, C, H, P, R> FromRef<AppState<Q, L, C, H, P, R>> for CalibrationState<C> {
    fn from_ref(input: &AppState<Q, L, C, H, P, R>) -> Self {
        CalibrationState(Arc::clone(&input.calibration))
    }
}

impl<Q, L, C, H, P, R> FromRef<AppState<Q, L, C, H, P, R>> for ReservoirState<R> {
    fn from_ref(input: &AppState<Q, L, C, H, P, R>) -> Self {
        ReservoirState(Arc::clone(&input.reservoir))
    }
}

pub fn router<Q, L, C, H, P, R>(
    state: AppState<Q, L, C, H, P, R>,
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
    R: ReservoirControl + 'static,
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
        .route("/api/reservoir", get(handlers::reservoir::status::<R>))
        .route(
            "/api/reservoir/settings",
            get(handlers::reservoir::settings::<R>).patch(handlers::reservoir::patch_settings::<R>),
        )
        .route(
            (
                "/api/reservoir",
                parse_path_segment::<OverrideTarget>(),
                "/override",
            ),
            put(handlers::reservoir::set_override::<R>)
                .delete(handlers::reservoir::clear_override::<R>),
        )
        .route(
            (
                "/api/reservoir",
                parse_path_segment::<ReservoirChannel>(),
                "/runtime/reset",
            ),
            post(handlers::reservoir::reset_runtime::<R>),
        )
        .route(
            "/api/reservoir/fill",
            post(handlers::reservoir::start_fill::<R>)
                .delete(handlers::reservoir::cancel_fill::<R>),
        )
        .route(
            "/api/reservoir/fault",
            delete(handlers::reservoir::clear_fault::<R>),
        )
        .layer(ApiLayer::new(token, cors_origin))
        .with_state(state)
}
