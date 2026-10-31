use std::sync::{Arc, Mutex};

use tokio::sync::{watch, Notify};
use tokio::time::Instant;

use crate::domain::light::{LightingOverride, LightingTarget};
use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::{LightingControl, TelemetryQuery};
use crate::ports::outbound::{LightingOverrideSource, TelemetrySink};

pub struct InMemoryTelemetryStore {
    tx: watch::Sender<TankTelemetryDto>,
}

impl InMemoryTelemetryStore {
    pub fn new() -> Self {
        Self {
            tx: watch::channel(TankTelemetryDto::empty()).0,
        }
    }
}

impl Default for InMemoryTelemetryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetrySink for InMemoryTelemetryStore {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.tx.send_replace(snapshot);
    }
}

impl TelemetryQuery for InMemoryTelemetryStore {
    fn snapshot(&self) -> TankTelemetryDto {
        self.tx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<TankTelemetryDto> {
        self.tx.subscribe()
    }
}

impl TelemetrySink for Arc<InMemoryTelemetryStore> {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.as_ref().publish(snapshot);
    }
}

struct StoredOverride {
    target: LightingTarget,
    expires_at: Option<Instant>,
}

pub struct InMemoryLightingStore {
    state: Mutex<Option<StoredOverride>>,
    notify: Arc<Notify>,
}

impl InMemoryLightingStore {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    fn read(&self) -> Option<LightingOverride> {
        let guard = self.state.lock().expect("mutex poisoned by a panic");
        let stored = guard.as_ref()?;

        let expires_in_s = match stored.expires_at {
            None => None,
            Some(at) => {
                let now = Instant::now();
                if now >= at {
                    return None;
                }
                Some(at.saturating_duration_since(now).as_secs() as u32)
            }
        };

        Some(LightingOverride {
            target: stored.target,
            expires_in_s,
        })
    }
}

impl Default for InMemoryLightingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LightingControl for InMemoryLightingStore {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        let expires_at =
            duration_s.map(|s| Instant::now() + std::time::Duration::from_secs(s as u64));

        *self.state.lock().expect("mutex poisoned by a panic") =
            Some(StoredOverride { target, expires_at });

        self.notify.notify_one();
    }

    fn clear_override(&self) {
        *self.state.lock().expect("mutex poisoned by a panic") = None;
        self.notify.notify_one();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingOverrideSource for InMemoryLightingStore {
    fn active_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingControl for Arc<InMemoryLightingStore> {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        self.as_ref().set_override(target, duration_s);
    }

    fn clear_override(&self) {
        self.as_ref().clear_override();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.as_ref().current_override()
    }
}

impl LightingOverrideSource for Arc<InMemoryLightingStore> {
    fn active_override(&self) -> Option<LightingOverride> {
        self.as_ref().active_override()
    }
}

#[cfg(test)]
mod tests;
