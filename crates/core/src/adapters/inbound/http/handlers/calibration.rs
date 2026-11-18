use picoserve::extract::State;
use picoserve::response::{Json, StatusCode};

use crate::adapters::inbound::http::routes::CalibrationState;
use crate::domain::calibration::{Calibration, CalibrationPatch};
use crate::ports::inbound::{CalibrationControl, CalibrationError};

pub async fn current<C: CalibrationControl>(
    State(CalibrationState(calibration)): State<CalibrationState<C>>,
) -> Json<Calibration> {
    tracing::info!("GET /api/calibration");

    Json(calibration.current())
}

pub async fn patch<C: CalibrationControl>(
    State(CalibrationState(calibration)): State<CalibrationState<C>>,
    Json(request): Json<CalibrationPatch>,
) -> Result<Json<Calibration>, (StatusCode, &'static str)> {
    match calibration.apply(request) {
        Ok(updated) => {
            tracing::info!("PATCH /api/calibration");
            Ok(Json(updated))
        }
        Err(CalibrationError::Invalid(reason)) => {
            tracing::warn!(reason, "PATCH /api/calibration rejected");
            Err((StatusCode::BAD_REQUEST, reason))
        }
        Err(CalibrationError::NotPersisted(reason)) => {
            tracing::error!(reason, "PATCH /api/calibration was not stored");
            Err((StatusCode::INTERNAL_SERVER_ERROR, reason))
        }
    }
}
