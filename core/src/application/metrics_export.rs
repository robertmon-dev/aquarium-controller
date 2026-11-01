use std::collections::VecDeque;

use crate::domain::metrics::TankTelemetryDto;
use crate::ports::outbound::metrics_export::{ExportError, MetricsWriter};
use crate::protocol::line_protocol;

pub struct MetricsExporter<W> {
    writer: W,
    pending: VecDeque<String>,
    capacity: usize,
    batch_max_snapshots: usize,
}

impl<W> MetricsExporter<W>
where
    W: MetricsWriter,
{
    pub fn new(writer: W, capacity: usize, batch_max_snapshots: usize) -> Self {
        Self {
            writer,
            pending: VecDeque::new(),
            capacity,
            batch_max_snapshots,
        }
    }

    pub fn enqueue(&mut self, snapshot: &TankTelemetryDto) -> bool {
        let Some(encoded) = line_protocol::encode(snapshot) else {
            return false;
        };

        if self.pending.len() == self.capacity {
            self.pending.pop_front();
        }

        self.pending.push_back(encoded);
        true
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
                self.drop_front(count);
                Ok(count)
            }
            Err(error) => {
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

#[cfg(test)]
mod tests;
