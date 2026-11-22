use serde::Serialize;

use super::service::{FinishedOperation, PumpService};
use super::state::{Activity, FinishWrite, State};
use crate::domain::pump::{Checkpoint, Operation};
use crate::ports::outbound::{Clock, PumpRecord, PumpStore};

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub fn checkpoint(&self) -> bool {
        let mut state = self.lock();
        state.pending.checkpoint = true;
        self.flush_pending(&mut state);
        !state.checkpoint_failed
    }

    pub fn persist_pending(&self) {
        let mut state = self.lock();
        self.flush_pending(&mut state);
    }

    pub(super) fn flush_pending(&self, state: &mut State) {
        let pending = std::mem::take(&mut state.pending);

        if let Err(reason) = self.flush_finish(state) {
            if let Some(finish) = state
                .pending_finish
                .as_mut()
                .filter(|finish| !finish.reported)
            {
                tracing::error!(reason, "pump completion is pending in storage, retrying");
                finish.reported = true;
            }
        }
        if pending.last {
            if let Err(reason) = self.write(PumpRecord::LastOperations, &state.last) {
                tracing::warn!(reason, "pump history will be retried");
                state.pending.last = true;
            }
        }

        if pending.checkpoint {
            if let Activity::Running(operation) = &state.activity {
                let written = self.write(
                    PumpRecord::Checkpoint,
                    &Checkpoint {
                        operation: operation.clone(),
                        checkpoint_at: self.clock.epoch_seconds(),
                    },
                );
                if let Err(reason) = written {
                    tracing::error!(reason, "pump checkpoint not stored, the drain waits for it");
                }
                state.checkpoint_failed = written.is_err();
            }
        }

        if pending.overrides {
            self.persist(PumpRecord::Overrides, &state.stored_overrides());
        }

        if pending.hold {
            if let Err(reason) = self.write(PumpRecord::ReturnHold, &state.return_held_after_drain)
            {
                tracing::warn!(reason, "return pump hold will be retried");
                state.pending.hold = true;
            }
        }

        if let Some(events) = &self.events {
            for event in pending.events {
                let _ = events.send(event);
            }
        }
    }

    fn flush_finish(&self, state: &mut State) -> Result<(), &'static str> {
        let Some(finish) = &mut state.pending_finish else {
            return Ok(());
        };
        if finish.next == FinishWrite::MarkFinished {
            let marker = FinishedOperation {
                finished: state.last,
                kind: finish.kind,
            };
            if self.write(PumpRecord::Checkpoint, &marker).is_ok() {
                finish.next = FinishWrite::CopyHistory;
            } else {
                self.write(PumpRecord::LastOperations, &state.last)?;
                finish.next = FinishWrite::InvalidateActive;
            }
        }

        if finish.next == FinishWrite::InvalidateActive {
            self.invalidate_checkpoint()?;
        } else {
            self.write(PumpRecord::LastOperations, &state.last)?;
            if let Err(reason) = self.store.erase(PumpRecord::Checkpoint) {
                tracing::warn!(reason, "archived pump completion remains in storage");
            }
        }
        state.pending_finish = None;
        Ok(())
    }

    fn invalidate_checkpoint(&self) -> Result<(), &'static str> {
        let Err(reason) = self.store.erase(PumpRecord::Checkpoint) else {
            return Ok(());
        };

        tracing::debug!(reason, "pump checkpoint not erased, writing a tombstone");
        self.write(PumpRecord::Checkpoint, &None::<Checkpoint>)
    }

    pub(super) fn record_ahead(
        &self,
        state: &mut State,
        operation: &Operation,
        epoch: Option<u64>,
    ) -> Result<(), &'static str> {
        if state
            .pending_finish
            .is_some_and(|finish| finish.next != FinishWrite::InvalidateActive)
        {
            self.write(PumpRecord::LastOperations, &state.last)?;
        }
        self.write(
            PumpRecord::Checkpoint,
            &Checkpoint {
                operation: operation.clone(),
                checkpoint_at: epoch,
            },
        )?;
        state.pending_finish = None;
        state.checkpoint_failed = false;
        Ok(())
    }

    pub fn flush_runtime(&self) {
        let state = self.lock();
        self.persist(PumpRecord::Runtime, &state.runtime());
    }

    pub(super) fn write<T: Serialize>(
        &self,
        record: PumpRecord,
        value: &T,
    ) -> Result<(), &'static str> {
        let bytes =
            serde_json::to_vec(value).map_err(|_| "the pump record could not be encoded")?;
        self.store.store(record, &bytes)
    }

    pub(super) fn persist<T: Serialize>(&self, record: PumpRecord, value: &T) {
        if let Err(reason) = self.write(record, value) {
            tracing::error!(record = record.key(), reason, "pump record not stored");
        }
    }
}

#[cfg(test)]
mod failure_tests;
#[cfg(test)]
mod tests;
