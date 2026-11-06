use std::time::Duration;

use picoserve::io::Write;
use picoserve::response::sse::{EventSource, EventWriter};
use picoserve::response::Json;
use tokio::sync::watch;

use crate::domain::metrics::TankTelemetryDto;

pub struct TelemetryEventSource {
    rx: watch::Receiver<TankTelemetryDto>,
    keepalive: Duration,
}

impl TelemetryEventSource {
    pub fn new(rx: watch::Receiver<TankTelemetryDto>, keepalive: Duration) -> Self {
        Self { rx, keepalive }
    }
}

impl EventSource for TelemetryEventSource {
    async fn write_events<W: Write>(
        mut self,
        mut writer: EventWriter<'_, W>,
    ) -> Result<(), W::Error> {
        let initial = self.rx.borrow_and_update().clone();
        writer.write_event("telemetry", Json(&initial)).await?;
        writer.flush().await?;

        loop {
            match tokio::time::timeout(self.keepalive, self.rx.changed()).await {
                Ok(Ok(())) => {
                    let snapshot = self.rx.borrow_and_update().clone();
                    writer.write_event("telemetry", Json(&snapshot)).await?;
                    writer.flush().await?;
                }
                Ok(Err(_)) => return Ok(()),
                Err(_) => writer.write_keepalive().await?,
            }
        }
    }
}
