use tokio::time::Instant;

use super::service::PumpService;
use super::state::{Activity, State};
use crate::domain::flood::FloodProbes;
use crate::domain::pump::OperationResult;
use crate::ports::inbound::FloodResetError;
use crate::ports::outbound::{Clock, LeakSource, PumpRecord, PumpStore};

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub fn observe_flood(&self, probes: FloodProbes) {
        let mut state = self.lock();
        let changed = state.flood.probes.replace(probes) != Some(probes);
        let tripped = probes.wet() && !state.flood.latched;
        if tripped {
            state.flood.latched = true;
            state.flood_pending = true;
        }
        if state.flood_pending {
            match self.write(PumpRecord::Flood, &true) {
                Ok(()) => state.flood_pending = false,
                Err(reason) if tripped => {
                    tracing::error!(reason, "flood alarm held in RAM, retrying storage")
                }
                Err(_) => {}
            }
        }
        self.abort_on_flood(&mut state);
        drop(state);
        if changed {
            if probes.wet() {
                tracing::error!(
                    reservoir = probes.reservoir,
                    sump = probes.sump,
                    "flood alarm"
                );
            }
            self.notify.notify_one();
        }
    }

    pub(super) fn abort_on_flood(&self, state: &mut State) -> bool {
        if !state.flood.latched {
            return false;
        }
        let operation = match &state.activity {
            Activity::Running(op) | Activity::Suspended(op) => op.clone(),
            Activity::Resuming(cp) => cp.operation.clone(),
            Activity::Idle => return false,
        };
        self.finish(
            state,
            &operation,
            OperationResult::Aborted,
            self.clock.epoch_seconds(),
            Instant::now(),
        );
        true
    }

    pub(super) fn acknowledge_flood(&self) -> Result<(), FloodResetError> {
        let mut state = self.lock();
        if !state.flood.latched {
            return Ok(());
        }
        match state.flood.probes {
            None => return Err(FloodResetError::Unavailable),
            Some(probes) if probes.wet() => return Err(FloodResetError::Wet),
            Some(_) => {}
        }
        self.flush_pending(&mut state);
        if state.invalidation_pending() {
            return Err(FloodResetError::Storage(
                "interrupted operation is still pending in storage",
            ));
        }
        self.write(PumpRecord::Flood, &false)
            .map_err(FloodResetError::Storage)?;
        state.flood.latched = false;
        state.flood_pending = false;
        drop(state);
        self.notify.notify_one();
        Ok(())
    }
}

impl<S: PumpStore, C: Clock> LeakSource for PumpService<S, C> {
    fn leak(&self) -> Option<bool> {
        let flood = self.lock().flood;
        if flood.latched {
            Some(true)
        } else {
            flood.probes.map(FloodProbes::wet)
        }
    }
}

#[cfg(test)]
mod tests;
