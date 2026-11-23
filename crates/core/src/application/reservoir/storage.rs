use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::domain::reservoir::{
    FaultRecord, FillFault, FillRecord, FillResult, FillTrigger, ReservoirSettings, RunningMarker,
};
use crate::ports::outbound::{ReservoirRecord, ReservoirStore};

enum Stored<T> {
    Absent,
    Unreadable,
    Value(T),
}

impl<T> Stored<T> {
    fn value(self) -> Option<T> {
        match self {
            Self::Value(value) => Some(value),
            Self::Absent | Self::Unreadable => None,
        }
    }
}

fn load<S: ReservoirStore, T: DeserializeOwned>(store: &S, record: ReservoirRecord) -> Stored<T> {
    let bytes = match store.load(record) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Stored::Absent,
        Err(reason) => {
            tracing::warn!(
                reason,
                record = record.key(),
                "stored barrel record could not be read"
            );
            return Stored::Unreadable;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(value) => Stored::Value(value),
        Err(_) => {
            tracing::warn!(record = record.key(), "stored barrel record is unreadable");
            Stored::Unreadable
        }
    }
}

pub(super) fn write<S: ReservoirStore, T: Serialize>(
    store: &S,
    record: ReservoirRecord,
    value: &T,
) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(value).map_err(|_| "the barrel record could not be encoded")?;
    store.store(record, &bytes)
}

pub(super) fn stored_settings<S: ReservoirStore>(
    store: &S,
    defaults: ReservoirSettings,
) -> ReservoirSettings {
    match load::<S, ReservoirSettings>(store, ReservoirRecord::Settings) {
        Stored::Value(stored) if stored.validate().is_ok() => stored,
        Stored::Absent => defaults,
        Stored::Value(_) | Stored::Unreadable => {
            tracing::warn!("stored barrel settings are invalid, using the defaults");
            defaults
        }
    }
}

pub(super) fn stored_runtime_ms<S: ReservoirStore>(store: &S) -> [u64; 4] {
    load::<S, [u64; 4]>(store, ReservoirRecord::Runtime)
        .value()
        .unwrap_or_default()
        .map(|seconds| seconds.saturating_mul(1000))
}

fn stored_fault<S: ReservoirStore>(store: &S) -> Option<FaultRecord> {
    match load::<S, Option<FaultRecord>>(store, ReservoirRecord::Fault) {
        Stored::Absent => None,
        Stored::Value(fault) => fault,
        Stored::Unreadable => {
            tracing::error!("the barrel fault record is unreadable, treating it as a fault");
            Some(FaultRecord {
                fault: FillFault::Unreadable,
                at: None,
            })
        }
    }
}

#[derive(Serialize, Deserialize)]
pub(super) struct SavedMarker {
    #[serde(default)]
    pub cycle_id: u64,
    #[serde(flatten)]
    pub marker: RunningMarker,
}

#[derive(Serialize, Deserialize)]
pub(super) struct SavedResult {
    #[serde(default)]
    pub cycle_id: u64,
    #[serde(flatten)]
    pub record: FillRecord,
    #[serde(default)]
    pub auto_latched: Option<bool>,
    #[serde(default)]
    pub held_after_reboot: bool,
}

pub(super) fn next_cycle_id(store: &impl ReservoirStore) -> Result<u64, &'static str> {
    #[derive(Deserialize)]
    struct Id {
        #[serde(default)]
        cycle_id: u64,
    }
    let previous = store
        .load(ReservoirRecord::Last)?
        .and_then(|bytes| serde_json::from_slice::<Id>(&bytes).ok())
        .map_or(0, |last| last.cycle_id);
    Ok(previous.wrapping_add(1).max(1))
}

pub(super) struct Recovery {
    pub(super) last: Option<FillRecord>,
    pub(super) cycle_id: u64,
    pub(super) auto_latched: bool,
    pub(super) fault: Option<FaultRecord>,
    pub(super) held_after_reboot: bool,
    pub(super) fault_pending: bool,
    pub(super) last_pending: bool,
    pub(super) invalidation_pending: bool,
}

pub(super) fn recover<S: ReservoirStore>(store: &S) -> Recovery {
    let saved = load::<S, SavedResult>(store, ReservoirRecord::Last);
    let unreadable = matches!(saved, Stored::Unreadable);
    let saved = saved.value();
    let mut recovery = Recovery {
        last: saved.as_ref().map(|saved| saved.record),
        cycle_id: saved.as_ref().map_or(0, |saved| saved.cycle_id),
        auto_latched: saved.as_ref().map_or(unreadable, |saved| {
            saved.auto_latched.unwrap_or_else(|| {
                saved.record.trigger == FillTrigger::Auto && saved.record.result.latches_auto()
            })
        }),
        fault: stored_fault(store),
        held_after_reboot: saved.as_ref().is_some_and(|saved| saved.held_after_reboot),
        fault_pending: false,
        last_pending: false,
        invalidation_pending: false,
    };
    let stored_marker = load::<S, SavedMarker>(store, ReservoirRecord::Running);
    let unknown_marker = matches!(stored_marker, Stored::Unreadable);
    let marker = match stored_marker {
        Stored::Absent => return recovery,
        Stored::Unreadable => SavedMarker {
            cycle_id: 0,
            marker: RunningMarker {
                trigger: FillTrigger::Auto,
                started_at: None,
            },
        },
        Stored::Value(marker) => marker,
    };
    if unreadable {
        recovery.held_after_reboot = true;
        recovery.fault.get_or_insert(FaultRecord {
            fault: FillFault::Unreadable,
            at: None,
        });
        return recovery;
    }
    if let Some(last) = recovery.last {
        let same_cycle = marker.cycle_id != 0 && marker.cycle_id == recovery.cycle_id;
        let legacy_match = marker.cycle_id == 0
            && recovery.cycle_id == 0
            && last.trigger == marker.marker.trigger
            && last.started_at == marker.marker.started_at;
        let possible_fault = unknown_marker && last.result.fault().is_some();
        if (same_cycle || legacy_match || possible_fault)
            && last.result != FillResult::InterruptedByReboot
        {
            if let (Some(fault), None) = (last.result.fault(), recovery.fault) {
                recovery.fault = Some(FaultRecord {
                    fault,
                    at: last.finished_at,
                });
                recovery.fault_pending = true;
            }
            recovery.held_after_reboot = !same_cycle;
            recovery.last_pending = recovery.held_after_reboot;
            recovery.invalidation_pending = true;
            return recovery;
        }
    }
    let interrupted = FillRecord::interrupted(marker.marker);
    recovery.held_after_reboot = true;
    recovery.auto_latched = false;
    recovery.last_pending =
        recovery.last != Some(interrupted) || recovery.cycle_id != marker.cycle_id;
    recovery.last = Some(interrupted);
    recovery.cycle_id = marker.cycle_id;
    tracing::warn!(
        "a barrel fill was interrupted by a restart, automatic fills wait for a manual start"
    );
    recovery
}
