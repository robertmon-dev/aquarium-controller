use tokio::time::Instant;

use super::service::PumpService;
use super::state::{Activity, State};
use crate::domain::flood::FloodProbes;
use crate::domain::pump::{OperationResult, PumpId};
use crate::ports::inbound::FloodResetError;
use crate::ports::outbound::{Clock, LeakSource, PumpOutputs, PumpRecord, PumpStore};

/// Open the relay bank when automation needs it or a flood cutoff must be held.
pub fn startup_pump_outputs<O: PumpOutputs, E>(
    configured: bool,
    hold_off: bool,
    open: impl FnOnce() -> Result<O, E>,
) -> Result<Option<O>, E> {
    if !configured && !hold_off {
        return Ok(None);
    }
    let mut outputs = open()?;
    if hold_off {
        for id in PumpId::ALL {
            if let Err(reason) = outputs.set(id, false) {
                tracing::error!(
                    pump = id.name(),
                    reason,
                    "failed to hold pump output off during startup"
                );
            }
        }
    }
    Ok(Some(outputs))
}

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
