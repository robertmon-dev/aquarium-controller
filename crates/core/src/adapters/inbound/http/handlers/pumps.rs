use picoserve::extract::State;
use picoserve::response::{Json, NoContent, StatusCode};
use serde::Deserialize;

use crate::adapters::inbound::http::routes::PumpState;
use crate::domain::pump::{
    Blocker, ChannelStatus, OperationKind, PumpId, PumpSettings, PumpSettingsPatch, PumpsStatus,
    WaterChangePlan,
};
use crate::ports::inbound::{OverrideError, PumpControl, PumpSettingsError, StartError};

type Rejection = (StatusCode, &'static str);
type Empty = (StatusCode, NoContent);

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SetPumpOverrideRequest {
    #[serde(default)]
    pub duration_s: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct StartTopOffRequest {
    pub duration_s: u32,
}

pub async fn status<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Json<PumpsStatus> {
    tracing::info!("GET /api/pumps");

    Json(pumps.status())
}

pub async fn settings<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Json<PumpSettings> {
    tracing::info!("GET /api/pumps/settings");

    Json(pumps.settings())
}

pub async fn patch_settings<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
    Json(patch): Json<PumpSettingsPatch>,
) -> Result<Json<PumpSettings>, Rejection> {
    match pumps.apply_settings(patch) {
        Ok(updated) => {
            tracing::info!("PATCH /api/pumps/settings");
            Ok(Json(updated))
        }
        Err(PumpSettingsError::Invalid(reason)) => {
            tracing::warn!(reason, "PATCH /api/pumps/settings rejected");
            Err((StatusCode::BAD_REQUEST, reason))
        }
        Err(PumpSettingsError::NotPersisted(reason)) => {
            tracing::error!(reason, "PATCH /api/pumps/settings was not stored");
            Err((StatusCode::INTERNAL_SERVER_ERROR, reason))
        }
    }
}

pub async fn set_override<P: PumpControl>(
    id: PumpId,
    State(PumpState(pumps)): State<PumpState<P>>,
    Json(request): Json<SetPumpOverrideRequest>,
) -> Result<Json<ChannelStatus>, Rejection> {
    tracing::info!(pump = id.name(), duration_s = ?request.duration_s, "PUT /api/pumps/{{id}}/override");

    pumps
        .set_override(id, request.duration_s)
        .map(Json)
        .map_err(|error| override_rejection(id, error))
}

pub async fn clear_override<P: PumpControl>(
    id: PumpId,
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Result<Empty, Rejection> {
    tracing::info!(pump = id.name(), "DELETE /api/pumps/{{id}}/override");

    pumps
        .clear_override(id)
        .map(|()| (StatusCode::NO_CONTENT, NoContent))
        .map_err(|error| override_rejection(id, error))
}

pub async fn reset_runtime<P: PumpControl>(
    id: PumpId,
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Empty {
    tracing::info!(pump = id.name(), "POST /api/pumps/{{id}}/runtime/reset");
    pumps.reset_runtime(id);
    (StatusCode::NO_CONTENT, NoContent)
}

pub async fn start_top_off<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
    Json(request): Json<StartTopOffRequest>,
) -> Result<Empty, Rejection> {
    accepted(
        "POST /api/pumps/top-off",
        pumps.start_top_off(request.duration_s),
    )
}

pub async fn resume_top_off<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Result<Empty, Rejection> {
    accepted(
        "POST /api/pumps/top-off/resume",
        pumps.resume(OperationKind::TopOff),
    )
}

pub async fn cancel_top_off<P: PumpControl>(State(PumpState(pumps)): State<PumpState<P>>) -> Empty {
    tracing::info!("DELETE /api/pumps/top-off");
    pumps.cancel(OperationKind::TopOff);
    (StatusCode::NO_CONTENT, NoContent)
}

pub async fn start_water_change<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
    Json(plan): Json<WaterChangePlan>,
) -> Result<Empty, Rejection> {
    accepted("POST /api/water-change", pumps.start_water_change(plan))
}

pub async fn resume_water_change<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Result<Empty, Rejection> {
    accepted(
        "POST /api/water-change/resume",
        pumps.resume(OperationKind::WaterChange),
    )
}

pub async fn cancel_water_change<P: PumpControl>(
    State(PumpState(pumps)): State<PumpState<P>>,
) -> Empty {
    tracing::info!("DELETE /api/water-change");
    pumps.cancel(OperationKind::WaterChange);
    (StatusCode::NO_CONTENT, NoContent)
}

fn override_rejection(id: PumpId, error: OverrideError) -> Rejection {
    match error {
        OverrideError::NotOverridable => {
            tracing::warn!(pump = id.name(), "override on a channel without overrides");
            (
                StatusCode::NOT_FOUND,
                "only the return pump and the UV lamp take overrides",
            )
        }
        OverrideError::Invalid(reason) => {
            tracing::warn!(pump = id.name(), reason, "override rejected");
            (StatusCode::BAD_REQUEST, reason)
        }
        OverrideError::NotPersisted(reason) => {
            tracing::error!(pump = id.name(), reason, "override not persisted");
            (StatusCode::INTERNAL_SERVER_ERROR, reason)
        }
    }
}

fn accepted(route: &'static str, result: Result<(), StartError>) -> Result<Empty, Rejection> {
    match result {
        Ok(()) => {
            tracing::info!(route, "accepted");
            Ok((StatusCode::ACCEPTED, NoContent))
        }
        Err(StartError::Invalid(reason)) => {
            tracing::warn!(route, reason, "rejected");
            Err((StatusCode::BAD_REQUEST, reason))
        }
        Err(StartError::Conflict) => {
            tracing::warn!(route, "conflict");
            Err((
                StatusCode::CONFLICT,
                "another pump operation is running, or none is suspended",
            ))
        }
        Err(StartError::Blocked(blocker)) => {
            tracing::warn!(route, ?blocker, "blocked");
            Err((StatusCode::LOCKED, blocker_reason(blocker)))
        }
    }
}

const fn blocker_reason(blocker: Blocker) -> &'static str {
    match blocker {
        Blocker::SumpLow => "the sump is low",
        Blocker::ReservoirEmpty => "the makeup reservoir is empty",
        Blocker::TankOverflow => "the tank overflow sensor is wet",
        Blocker::LevelsUnavailable => "water level readings are unavailable",
        Blocker::ReturnOff => "the return pump is off",
        Blocker::SumpFull => "the sump is full",
        Blocker::RecentWaterChange => "the previous water change was too recent",
        Blocker::ReservoirLow => "the makeup reservoir does not hold enough for a water change",
        Blocker::WaterChange => "a water change is running",
    }
}
