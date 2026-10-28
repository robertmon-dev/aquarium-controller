use picoserve::extract::State;
use picoserve::response::{Json, StatusCode};
use serde::Deserialize;

use crate::adapters::inbound::http::routes::LightingState;
use crate::domain::light::{LightingOverride, LightingTarget};
use crate::ports::inbound::LightingControl;

#[derive(Debug, Deserialize)]
pub struct SetOverrideRequest {
    pub target: LightingTarget,
    pub duration_s: Option<u32>,
}

impl SetOverrideRequest {
    fn validate(&self) -> Result<(), &'static str> {
        self.target.validate()?;

        if self.duration_s == Some(0) {
            return Err("duration must be positive");
        }

        Ok(())
    }
}

pub async fn current<L: LightingControl>(
    State(LightingState(lighting)): State<LightingState<L>>,
) -> Result<Json<LightingOverride>, StatusCode> {
    tracing::info!("GET /api/lighting/override");

    match lighting.current_override() {
        Some(over) => Ok(Json(over)),
        None => Err(StatusCode::NO_CONTENT),
    }
}

pub async fn set<L: LightingControl>(
    State(LightingState(lighting)): State<LightingState<L>>,
    Json(request): Json<SetOverrideRequest>,
) -> Result<Json<LightingOverride>, (StatusCode, &'static str)> {
    if let Err(reason) = request.validate() {
        tracing::warn!(reason, "PUT /api/lighting/override rejected");
        return Err((StatusCode::BAD_REQUEST, reason));
    }

    lighting.set_override(request.target, request.duration_s);
    tracing::info!(?request.target, "PUT /api/lighting/override");

    Ok(Json(lighting.current_override().expect(
        "override was just set and duration_s > 0 is enforced",
    )))
}

pub async fn clear<L: LightingControl>(
    State(LightingState(lighting)): State<LightingState<L>>,
) -> StatusCode {
    tracing::info!("DELETE /api/lighting/override");
    lighting.clear_override();
    StatusCode::NO_CONTENT
}
