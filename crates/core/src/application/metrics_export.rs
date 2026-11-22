use std::collections::VecDeque;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::domain::metrics::TankTelemetryDto;
use crate::domain::pump::OperationEvent;
use crate::ports::outbound::metrics_export::{ExportError, MetricsWriter};
use crate::protocol::line_protocol;

pub struct MetricsExporter<W> {
    writer: W,
    pending: VecDeque<String>,
    capacity: usize,
    batch_max_snapshots: usize,
    retry_backoff: Duration,
    next_attempt_at: Option<Instant>,
}

impl<W> MetricsExporter<W>
where
    W: MetricsWriter,
{
    pub fn new(
        writer: W,
        capacity: usize,
        batch_max_snapshots: usize,
        retry_backoff: Duration,
    ) -> Self {
        Self {
            writer,
            pending: VecDeque::new(),
            capacity,
            batch_max_snapshots,
            retry_backoff,
            next_attempt_at: None,
        }
    }

    pub fn enqueue(&mut self, snapshot: &TankTelemetryDto) -> bool {
        self.push(line_protocol::encode(snapshot))
    }

    pub fn enqueue_operation(&mut self, event: &OperationEvent) -> bool {
        self.push(line_protocol::encode_operation(event))
    }

    fn push(&mut self, encoded: Option<String>) -> bool {
        let Some(encoded) = encoded else {
            return false;
        };

        if self.pending.len() == self.capacity {
            self.pending.pop_front();
        }

        self.pending.push_back(encoded);
        true
    }

    pub fn next_attempt_at(&self) -> Option<Instant> {
        self.next_attempt_at.filter(|_| !self.pending.is_empty())
    }

    pub fn ready(&self, now: Instant) -> bool {
        !self.pending.is_empty() && self.next_attempt_at.is_none_or(|at| now >= at)
    }

    pub async fn flush_ready(&mut self) {
        while self.ready(Instant::now()) {
            match self.flush().await {
                Ok(0) | Err(_) => break,
                Ok(_) => {}
            }
        }
    }

    pub async fn flush(&mut self) -> Result<usize, ExportError> {
        let count = self.pending.len().min(self.batch_max_snapshots);
        if count == 0 {
            return Ok(0);
        }

        let body = self
            .pending
            .iter()
            .take(count)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n");

        match self.writer.write(&body).await {
            Ok(()) => {
                self.next_attempt_at = None;
                self.drop_front(count);
                Ok(count)
            }
            Err(error) => {
                self.next_attempt_at = Some(Instant::now() + self.retry_backoff);
                if error.is_permanent() {
                    self.drop_front(count);
                }
                Err(error)
            }
        }
    }

    fn drop_front(&mut self, count: usize) {
        self.pending.drain(..count);
    }

    pub fn pending(&self) -> usize {
        self.pending.len()
    }
}

pub async fn export_loop<W: MetricsWriter>(
    mut exporter: MetricsExporter<W>,
    mut snapshots: watch::Receiver<TankTelemetryDto>,
    mut events: mpsc::UnboundedReceiver<OperationEvent>,
) {
    loop {
        let retry_at = exporter.next_attempt_at();

        tokio::select! {
            changed = snapshots.changed() => {
                if changed.is_err() {
                    return;
                }

                let snapshot = snapshots.borrow_and_update().clone();
                exporter.enqueue(&snapshot);
            }
            Some(event) = events.recv() => {
                exporter.enqueue_operation(&event);
            }
            () = sleep_until_some(retry_at), if retry_at.is_some() => {}
        }

        exporter.flush_ready().await;
    }
}

async fn sleep_until_some(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => core::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
