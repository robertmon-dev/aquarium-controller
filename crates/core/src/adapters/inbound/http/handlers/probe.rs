use std::sync::Arc;

use picoserve::extract::State;
use picoserve::response::Json;

use crate::domain::metrics::ProbeVoltagesDto;
use crate::ports::inbound::ProbeVoltageQuery;

pub async fn voltages<Q: ProbeVoltageQuery>(
    State(probes): State<Arc<Q>>,
) -> Json<ProbeVoltagesDto> {
    tracing::info!("GET /api/probe");
    Json(probes.probe_voltages())
}
