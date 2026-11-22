use picoserve::extract::State;
use picoserve::response::{Json, NoContent, StatusCode};
use serde::Deserialize;

use crate::adapters::inbound::http::routes::ReservoirState;
use crate::domain::reservoir::{
    OverrideTarget, ReservoirBlocker, ReservoirChannel, ReservoirSettings, ReservoirSettingsPatch,
    ReservoirStatus,
};
use crate::ports::inbound::{
    FaultClearError, FillCancelError, FillStartError, ReservoirControl, ReservoirOverrideError,
    ReservoirSettingsError,
};

type Rejection = (StatusCode, &'static str);
type Empty = (StatusCode, NoContent);

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SetReservoirOverrideRequest {
    pub on: bool,
    #[serde(default)]
    pub duration_s: Option<u32>,
}

pub async fn status<R: ReservoirControl>(
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Json<ReservoirStatus> {
    tracing::info!("GET /api/reservoir");

    Json(reservoir.status())
}

pub async fn settings<R: ReservoirControl>(
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Json<ReservoirSettings> {
    tracing::info!("GET /api/reservoir/settings");

    Json(reservoir.settings())
}

pub async fn patch_settings<R: ReservoirControl>(
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
    Json(patch): Json<ReservoirSettingsPatch>,
) -> Result<Json<ReservoirSettings>, Rejection> {
    match reservoir.apply_settings(patch) {
        Ok(updated) => {
            tracing::info!("PATCH /api/reservoir/settings");
            Ok(Json(updated))
        }
        Err(ReservoirSettingsError::Invalid(reason)) => {
            tracing::warn!(reason, "PATCH /api/reservoir/settings rejected");
            Err((StatusCode::BAD_REQUEST, reason))
        }
        Err(ReservoirSettingsError::NotPersisted(reason)) => {
            tracing::error!(reason, "PATCH /api/reservoir/settings was not stored");
            Err((StatusCode::SERVICE_UNAVAILABLE, reason))
        }
    }
}

pub async fn set_override<R: ReservoirControl>(
    target: OverrideTarget,
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
    Json(request): Json<SetReservoirOverrideRequest>,
) -> Result<Json<ReservoirStatus>, Rejection> {
    tracing::info!(target = target.name(), on = request.on, duration_s = ?request.duration_s, "PUT /api/reservoir/{{target}}/override");

    reservoir
        .set_override(target, request.on, request.duration_s)
        .map(Json)
        .map_err(|error| {
            tracing::warn!(
                target = target.name(),
                ?error,
                "PUT /api/reservoir/{{target}}/override refused"
            );
            match error {
                ReservoirOverrideError::Invalid(reason) => (StatusCode::BAD_REQUEST, reason),
                ReservoirOverrideError::Busy => (
                    StatusCode::CONFLICT,
                    "a barrel fill is running; cancel it before overriding",
                ),
                ReservoirOverrideError::Blocked(blocker) => {
                    (StatusCode::LOCKED, blocker_reason(blocker))
                }
            }
        })
}

pub async fn clear_override<R: ReservoirControl>(
    target: OverrideTarget,
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Empty {
    tracing::info!(
        target = target.name(),
        "DELETE /api/reservoir/{{target}}/override"
    );

    reservoir.clear_override(target);
    (StatusCode::NO_CONTENT, NoContent)
}

pub async fn reset_runtime<R: ReservoirControl>(
    channel: ReservoirChannel,
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Empty {
    tracing::info!(
        channel = channel.name(),
        "POST /api/reservoir/{{channel}}/runtime/reset"
    );

    reservoir.reset_runtime(channel);
    (StatusCode::NO_CONTENT, NoContent)
}

pub async fn start_fill<R: ReservoirControl>(
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Result<Empty, Rejection> {
    match reservoir.start_fill() {
        Ok(()) => {
            tracing::info!("POST /api/reservoir/fill accepted");
            Ok((StatusCode::ACCEPTED, NoContent))
        }
        Err(FillStartError::Conflict) => {
            tracing::warn!("POST /api/reservoir/fill refused: already running");
            Err((StatusCode::CONFLICT, "a barrel fill is already running"))
        }
        Err(FillStartError::Blocked(blocker)) => {
            tracing::warn!(?blocker, "POST /api/reservoir/fill blocked");
            Err((StatusCode::LOCKED, blocker_reason(blocker)))
        }
        Err(FillStartError::NotPersisted(reason)) => {
            tracing::error!(reason, "POST /api/reservoir/fill not recorded");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "the fill could not be recorded",
            ))
        }
    }
}

pub async fn cancel_fill<R: ReservoirControl>(
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Result<Empty, Rejection> {
    match reservoir.cancel_fill() {
        Ok(()) => {
            tracing::info!("DELETE /api/reservoir/fill");
            Ok((StatusCode::NO_CONTENT, NoContent))
        }
        Err(FillCancelError::NotPersisted(reason)) => {
            tracing::error!(reason, "DELETE /api/reservoir/fill not recorded");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "cancelled, but not recorded; the controller keeps retrying",
            ))
        }
    }
}

pub async fn clear_fault<R: ReservoirControl>(
    State(ReservoirState(reservoir)): State<ReservoirState<R>>,
) -> Result<Empty, Rejection> {
    match reservoir.clear_fault() {
        Ok(()) => {
            tracing::info!("DELETE /api/reservoir/fault");
            Ok((StatusCode::NO_CONTENT, NoContent))
        }
        Err(FaultClearError::Blocked(blocker)) => {
            tracing::warn!(?blocker, "DELETE /api/reservoir/fault blocked");
            Err((StatusCode::LOCKED, blocker_reason(blocker)))
        }
        Err(FaultClearError::NotPersisted(reason)) => {
            tracing::error!(reason, "DELETE /api/reservoir/fault not stored");
            Err((
                StatusCode::SERVICE_UNAVAILABLE,
                "the fault could not be cleared in storage",
            ))
        }
    }
}

const fn blocker_reason(blocker: ReservoirBlocker) -> &'static str {
    match blocker {
        ReservoirBlocker::SupervisionUnavailable => {
            "the controller watchdog is not supervising the barrel"
        }
        ReservoirBlocker::LevelsUnavailable => "barrel level readings are unavailable",
        ReservoirBlocker::Leak => "the leak sensor is wet",
        ReservoirBlocker::Fault => {
            "the barrel fill is locked by a fault; clear it with DELETE /api/reservoir/fault"
        }
        ReservoirBlocker::Interlock => "the other inlet valve may still be open",
        ReservoirBlocker::ReservoirFull => "the barrel is full",
        ReservoirBlocker::WaterChange => "a water change is running; the barrel fills after it",
        ReservoirBlocker::DutyRest => "the inlet is resting after a long run",
        ReservoirBlocker::ReservoirEmpty => "the barrel is empty, the mixer would run dry",
        ReservoirBlocker::TdsUnavailable => "no fresh TDS reading for the target",
    }
}
