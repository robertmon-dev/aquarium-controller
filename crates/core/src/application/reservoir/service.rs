use std::sync::{Arc, Mutex, MutexGuard};

use tokio::sync::{mpsc, Notify};

use super::state::{Output, State};
use super::storage::{recover, stored_runtime_ms, stored_settings, write};
use super::timing::ReservoirTiming;
use crate::domain::pump::Supervision;
use crate::domain::reservoir::{FillEvent, ReservoirBlocker, ReservoirChannel, ReservoirSettings};
use crate::ports::outbound::{Clock, ReservoirRecord, ReservoirStore};

pub struct ReservoirService<S, C> {
    pub(super) state: Mutex<State>,
    pub(super) store: S,
    pub(super) clock: C,
    pub(super) notify: Arc<Notify>,
    pub(super) events: Option<mpsc::UnboundedSender<FillEvent>>,
    pub(super) timing: ReservoirTiming,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Recorded {
    pub(super) changed: bool,
    pub(super) on_changed: bool,
}

impl<S: ReservoirStore, C: Clock> ReservoirService<S, C> {
    pub fn new(
        defaults: ReservoirSettings,
        store: S,
        clock: C,
        events: Option<mpsc::UnboundedSender<FillEvent>>,
        timing: ReservoirTiming,
    ) -> Self {
        let state = State::new(
            stored_settings(&store, defaults),
            stored_runtime_ms(&store),
            recover(&store),
        );
        let service = Self {
            state: Mutex::new(state),
            store,
            clock,
            notify: Arc::new(Notify::new()),
            events,
            timing,
        };
        service.persist_pending();
        service
    }

    pub fn set_supervision(&self, supervision: Supervision) {
        let changed = std::mem::replace(&mut self.lock().supervision, supervision) != supervision;
        if changed {
            tracing::info!(?supervision, "barrel supervision");
            self.notify.notify_one();
        }
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub(super) fn next_override_expiry(&self) -> Option<tokio::time::Instant> {
        self.lock()
            .overrides
            .iter()
            .flatten()
            .filter_map(|slot| slot.until)
            .min()
    }

    pub(super) fn record(
        &self,
        channel: ReservoirChannel,
        on: bool,
        error: bool,
        blocked_by: Option<ReservoirBlocker>,
    ) -> Recorded {
        let next = Output {
            on,
            error,
            blocked_by,
        };
        let previous = std::mem::replace(&mut self.lock().outputs[channel.index()], next);
        Recorded {
            changed: previous != next,
            on_changed: previous.on != next.on,
        }
    }

    pub(super) fn persist_pending(&self) {
        let mut state = self.lock();
        self.flush_pending(&mut state);
    }

    pub(super) fn flush_runtime(&self) {
        let seconds = self.lock().runtime_ms.map(|ms| ms / 1000);
        if let Err(reason) = write(&self.store, ReservoirRecord::Runtime, &seconds) {
            tracing::warn!(reason, "barrel runtime not stored");
        }
    }

    pub(super) fn flush_pending(&self, state: &mut State) {
        if state.fault_pending && write(&self.store, ReservoirRecord::Fault, &state.fault).is_ok() {
            state.fault_pending = false;
            tracing::info!("barrel fault stored");
        }
        if state.last_pending {
            match write(&self.store, ReservoirRecord::Last, &state.last) {
                Ok(()) => state.last_pending = false,
                Err(reason) => tracing::warn!(reason, "barrel fill result not stored, retrying"),
            }
        }
        if state.invalidation_pending
            && !state.last_pending
            && !state.fault_pending
            && self.store.erase(ReservoirRecord::Running).is_ok()
        {
            state.invalidation_pending = false;
        }
        let events = std::mem::take(&mut state.events);
        if let Some(sender) = &self.events {
            for event in events {
                let _ = sender.send(event);
            }
        }
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("reservoir mutex poisoned")
    }
}

#[cfg(test)]
mod storage_tests;
#[cfg(test)]
mod tests;
