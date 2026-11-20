use picoserve::extract::State;
use picoserve::response::{Json, NoContent, StatusCode};
use serde::Deserialize;

use crate::adapters::inbound::http::routes::HeaterState;
use crate::domain::heater::{
    HeaterId, HeaterSettings, HeaterSettingsPatch, HeaterStatus, HeatersStatus, OverrideState,
};
use crate::ports::inbound::{HeaterControl, HeaterSettingsError};

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SetHeaterOverrideRequest {
    pub state: OverrideState,

    #[serde(default)]
    pub duration_s: Option<u32>,
}

pub async fn status<H: HeaterControl>(
    State(HeaterState(heaters)): State<HeaterState<H>>,
) -> Json<HeatersStatus> {
    tracing::info!("GET /api/heaters");

    Json(heaters.status())
}

pub async fn settings<H: HeaterControl>(
    State(HeaterState(heaters)): State<HeaterState<H>>,
) -> Json<HeaterSettings> {
    tracing::info!("GET /api/heaters/settings");

    Json(heaters.settings())
}

pub async fn patch_settings<H: HeaterControl>(
    State(HeaterState(heaters)): State<HeaterState<H>>,
    Json(patch): Json<HeaterSettingsPatch>,
) -> Result<Json<HeaterSettings>, (StatusCode, &'static str)> {
    match heaters.apply_settings(patch) {
        Ok(updated) => {
            tracing::info!("PATCH /api/heaters/settings");
            Ok(Json(updated))
        }
        Err(HeaterSettingsError::Invalid(reason)) => {
            tracing::warn!(reason, "PATCH /api/heaters/settings rejected");
            Err((StatusCode::BAD_REQUEST, reason))
        }
        Err(HeaterSettingsError::NotPersisted(reason)) => {
            tracing::error!(reason, "PATCH /api/heaters/settings was not stored");
            Err((StatusCode::INTERNAL_SERVER_ERROR, reason))
        }
    }
}

pub async fn set_override<H: HeaterControl>(
    id: HeaterId,
    State(HeaterState(heaters)): State<HeaterState<H>>,
    Json(request): Json<SetHeaterOverrideRequest>,
) -> Result<Json<HeaterStatus>, (StatusCode, &'static str)> {
    match heaters.set_override(id, request.state, request.duration_s) {
        Ok(status) => {
            tracing::info!(heater = id.name(), ?request.state, "PUT /api/heaters/{{id}}/override");
            Ok(Json(status))
        }
        Err(reason) => {
            tracing::warn!(
                heater = id.name(),
                reason,
                "PUT /api/heaters/{{id}}/override rejected"
            );
            Err((StatusCode::BAD_REQUEST, reason))
        }
    }
}

pub async fn clear_override<H: HeaterControl>(
    id: HeaterId,
    State(HeaterState(heaters)): State<HeaterState<H>>,
) -> (StatusCode, NoContent) {
    tracing::info!(heater = id.name(), "DELETE /api/heaters/{{id}}/override");
    heaters.clear_override(id);
    (StatusCode::NO_CONTENT, NoContent)
}
