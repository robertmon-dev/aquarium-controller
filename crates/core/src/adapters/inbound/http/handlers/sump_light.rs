use picoserve::extract::State;
use picoserve::response::{Json, StatusCode};
use serde::Deserialize;

use crate::adapters::inbound::http::routes::SumpLightState;
use crate::domain::sump_light::SumpLightStatus;
use crate::ports::inbound::SumpLightControl;

#[derive(Debug, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SetSumpLightRequest {
    pub on: bool,

    #[serde(default)]
    pub duration_s: Option<u32>,
}

pub async fn status<S: SumpLightControl>(
    State(SumpLightState(sump_light)): State<SumpLightState<S>>,
) -> Json<SumpLightStatus> {
    tracing::info!("GET /api/sump-light");

    Json(sump_light.status())
}

pub async fn set<S: SumpLightControl>(
    State(SumpLightState(sump_light)): State<SumpLightState<S>>,
    Json(request): Json<SetSumpLightRequest>,
) -> Result<Json<SumpLightStatus>, (StatusCode, &'static str)> {
    match sump_light.set(request.on, request.duration_s) {
        Ok(status) => {
            tracing::info!(on = request.on, "PUT /api/sump-light");
            Ok(Json(status))
        }
        Err(reason) => {
            tracing::warn!(reason, "PUT /api/sump-light rejected");
            Err((StatusCode::BAD_REQUEST, reason))
        }
    }
}
