use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::domain::reservoir::{
    FaultRecord, FillFault, FillRecord, FillTrigger, ReservoirSettings, RunningMarker,
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

fn finished(last: &FillRecord, marker: &RunningMarker) -> bool {
    marker.started_at.is_some()
        && last.started_at == marker.started_at
        && last.trigger == marker.trigger
        && last.finished_at.is_some()
}

pub(super) struct Recovery {
    pub(super) last: Option<FillRecord>,
    pub(super) fault: Option<FaultRecord>,
    pub(super) held_after_reboot: bool,
    pub(super) fault_pending: bool,
    pub(super) invalidation_pending: bool,
}

pub(super) fn recover<S: ReservoirStore>(store: &S) -> Recovery {
    let mut recovery = Recovery {
        last: load::<S, FillRecord>(store, ReservoirRecord::Last).value(),
        fault: stored_fault(store),
        held_after_reboot: false,
        fault_pending: false,
        invalidation_pending: false,
    };
    let marker = match load::<S, RunningMarker>(store, ReservoirRecord::Running) {
        Stored::Absent => return recovery,
        Stored::Unreadable => RunningMarker {
            trigger: FillTrigger::Auto,
            started_at: None,
        },
        Stored::Value(marker) => marker,
    };

    match recovery.last.filter(|last| finished(last, &marker)) {
        Some(last) => {
            if let (Some(fault), None) = (last.result.fault(), recovery.fault) {
                tracing::error!(?fault, "restoring a barrel fault that was not stored");
                recovery.fault = Some(FaultRecord {
                    fault,
                    at: last.finished_at,
                });
                recovery.fault_pending = true;
            }
            recovery.invalidation_pending = true;
        }
        None => {
            let record = FillRecord::interrupted(marker);
            if recovery.last != Some(record) {
                if let Err(reason) = write(store, ReservoirRecord::Last, &record) {
                    tracing::warn!(reason, "interrupted barrel fill not recorded");
                }
                recovery.last = Some(record);
            }
            recovery.held_after_reboot = true;
            tracing::warn!(
                "a barrel fill was interrupted by a restart, automatic fills wait for a manual start"
            );
        }
    }
    recovery
}
