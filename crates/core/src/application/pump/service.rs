use std::sync::{Arc, Mutex, MutexGuard};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, Notify};
use tokio::time::Instant;

use super::schedule::Schedule;
use super::state::{Activity, Expiry, FinishWrite, Pending, PendingFinish, State};
use crate::domain::pump::{
    Checkpoint, LastOperations, OperationEvent, OperationKind, OperationRecord, PumpId,
    PumpSettings, RuntimeCounters, StoredOverride, Supervision,
};
use crate::domain::water::WaterLevelRole;
use crate::ports::outbound::{Clock, PumpRecord, PumpStore};

#[derive(Serialize, Deserialize)]
pub(super) struct FinishedOperation {
    pub(super) finished: LastOperations,
    pub(super) kind: OperationKind,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum StoredOperation {
    Active(Checkpoint),
    Finished(FinishedOperation),
}

pub struct PumpService<S, C> {
    pub(super) state: Mutex<State>,
    pub(super) store: S,
    pub(super) clock: C,
    pub(super) notify: Arc<Notify>,
    pub(super) events: Option<mpsc::UnboundedSender<OperationEvent>>,
}

fn load<S: PumpStore, T: DeserializeOwned>(store: &S, record: PumpRecord) -> Option<T> {
    let bytes = store.load(record)?;

    match serde_json::from_slice(&bytes) {
        Ok(value) => Some(value),
        Err(_) => {
            tracing::warn!(record = record.key(), "stored pump record is unreadable");
            None
        }
    }
}

fn stored_settings<S: PumpStore>(store: &S, defaults: PumpSettings) -> PumpSettings {
    let Some(stored) = load::<S, PumpSettings>(store, PumpRecord::Settings) else {
        return defaults;
    };

    match stored.validate() {
        Ok(()) => stored,
        Err(reason) => {
            tracing::warn!(
                record = PumpRecord::Settings.key(),
                reason,
                "stored pump settings are invalid, using the defaults"
            );
            defaults
        }
    }
}

fn entry<T: DeserializeOwned>(record: PumpRecord, value: &serde_json::Value) -> Option<T> {
    match Option::<T>::deserialize(value) {
        Ok(entry) => entry,
        Err(_) => {
            tracing::warn!(
                record = record.key(),
                "stored pump entry is unreadable, dropping it"
            );
            None
        }
    }
}

fn stored_overrides<S: PumpStore>(store: &S) -> [Option<Expiry>; 2] {
    let mut slots = [None; 2];
    let Some(entries) = load::<S, Vec<serde_json::Value>>(store, PumpRecord::Overrides) else {
        return slots;
    };

    for (slot, value) in slots.iter_mut().zip(&entries) {
        *slot = entry::<StoredOverride>(PumpRecord::Overrides, value)
            .map(|stored| stored.expires_at.map_or(Expiry::Never, Expiry::Epoch));
    }

    slots
}

fn stored_history<S: PumpStore>(store: &S) -> LastOperations {
    let record = PumpRecord::LastOperations;
    let Some(value) = load::<S, serde_json::Value>(store, record) else {
        return LastOperations::default();
    };

    let Some(fields) = value.as_object() else {
        tracing::warn!(record = record.key(), "stored pump record is unreadable");
        return LastOperations::default();
    };

    let field = |name: &str| fields.get(name).unwrap_or(&serde_json::Value::Null);

    let water_change: Option<OperationRecord> = entry(record, field("water_change"));
    let drained_at = if fields.contains_key("drained_at") {
        entry(record, field("drained_at"))
    } else {
        water_change
            .filter(|record| record.drain_ran_s > 0)
            .and_then(|record| record.started_at)
    };

    LastOperations {
        top_off: entry(record, field("top_off")),
        water_change,
        drained_at,
    }
}

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub fn new(
        defaults: PumpSettings,
        store: S,
        clock: C,
        events: Option<mpsc::UnboundedSender<OperationEvent>>,
    ) -> Self {
        let settings = stored_settings(&store, defaults);
        let overrides = stored_overrides(&store);
        let runtime = load::<S, RuntimeCounters>(&store, PumpRecord::Runtime).unwrap_or_default();
        let return_held_after_drain =
            load::<S, bool>(&store, PumpRecord::ReturnHold).unwrap_or(false);
        let stored = load::<S, Option<StoredOperation>>(&store, PumpRecord::Checkpoint).flatten();
        let (activity, last, pending_finish) = match stored {
            Some(StoredOperation::Finished(record)) => (
                Activity::Idle,
                record.finished,
                Some(PendingFinish {
                    kind: record.kind,
                    next: FinishWrite::CopyHistory,
                    reported: false,
                }),
            ),
            Some(StoredOperation::Active(checkpoint)) => {
                (Activity::Resuming(checkpoint), stored_history(&store), None)
            }
            None => (Activity::Idle, stored_history(&store), None),
        };
        let resuming = match &activity {
            Activity::Resuming(checkpoint) => Some(&checkpoint.operation),
            _ => None,
        };
        let started_before_boot = [OperationKind::TopOff, OperationKind::WaterChange].map(|kind| {
            let resumed = resuming
                .filter(|operation| operation.plan.kind == kind)
                .and_then(|operation| operation.started_at);
            let recorded = match kind {
                OperationKind::TopOff => last.top_off,
                OperationKind::WaterChange => last.water_change,
            }
            .and_then(|record| record.started_at);
            resumed.max(recorded)
        });

        Self {
            state: Mutex::new(State {
                schedule: Schedule::compile(&settings, None),
                settings,
                overrides,
                pending: Pending::default(),
                activity,
                fresh_operation: false,
                levels: None,
                outputs: [None; 4],
                runtime_ms: runtime.runtime_s.map(|seconds| seconds * 1000),
                reset_at: runtime.reset_at,
                reset_instant: [None; 4],
                last,
                last_skipped: None,
                last_fired_minute: None,
                resume_sync_epoch: None,
                circulation_starved: false,
                level_sensors_suspect: false,
                pending_finish,
                water_change_ended: None,
                return_held_after_drain,
                supervision: Supervision::Off,
                checkpoint_failed: false,
                started_before_boot,
            }),
            store,
            clock,
            notify: Arc::new(Notify::new()),
            events,
        }
    }

    pub fn set_supervision(&self, supervision: Supervision) {
        let changed = {
            let mut state = self.lock();
            std::mem::replace(&mut state.supervision, supervision) != supervision
        };

        if changed {
            match supervision {
                Supervision::Unavailable => {
                    tracing::error!("supervision unavailable, new drain and makeup runs refused");
                }
                _ => tracing::info!(?supervision, "supervision"),
            }
            self.notify.notify_one();
        }
    }

    pub fn refill_stop_sensor(&self) -> Option<WaterLevelRole> {
        self.lock().settings.refill_stop_sensor
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub(super) fn reset_runtime_at(&self, id: PumpId, now: Instant) {
        let epoch = self.clock.epoch_seconds();

        {
            let mut state = self.lock();
            state.runtime_ms[id.index()] = 0;
            state.reset_at[id.index()] = epoch;
            state.reset_instant[id.index()] = Some(now);
            self.persist(PumpRecord::Runtime, &state.runtime());
        }

        self.notify.notify_one();
    }

    pub fn report_circulation(&self, starved: bool) -> bool {
        let mut state = self.lock();
        let changed = state.circulation_starved != starved;
        state.circulation_starved = starved;

        if changed && starved {
            tracing::warn!("the sump keeps running dry, circulation is starved");
        } else if changed {
            tracing::info!("the sump stayed wet, circulation recovered");
        }

        changed
    }

    pub fn report_level_sensors(&self, suspect: bool) -> bool {
        let mut state = self.lock();
        let changed = state.level_sensors_suspect != suspect;
        state.level_sensors_suspect = suspect;

        if changed && suspect {
            tracing::warn!("the level sensor harness looks unplugged or shorted, ignoring it");
        } else if changed {
            tracing::info!("the level sensor harness reads plausibly again");
        }

        changed
    }

    pub(super) fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("pump mutex poisoned")
    }
}

#[cfg(test)]
mod tests;
