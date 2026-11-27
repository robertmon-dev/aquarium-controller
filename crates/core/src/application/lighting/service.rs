use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use crate::domain::lighting::{
    LightingOverride, LightingPalette, LightingPalettePatch, LightingTarget,
};
use crate::domain::metrics::LightingMetricsDto;
use crate::ports::inbound::{LightingControl, LightingPaletteError};
use crate::ports::outbound::{LightingPersistence, LightingStatusSource};

#[derive(Debug, Clone, Copy)]
struct StoredOverride {
    target: LightingTarget,
    expires_at: Option<Instant>,
}

struct State {
    palette: LightingPalette,
    over: Option<StoredOverride>,
    status: LightingMetricsDto,
}

pub struct LightingService<P> {
    state: Mutex<State>,
    persistence: P,
    notify: Arc<Notify>,
}

impl<P: LightingPersistence> LightingService<P> {
    pub fn new(defaults: LightingPalette, persistence: P) -> Self {
        let palette = persistence
            .load()
            .filter(|stored| stored.validate().is_ok())
            .unwrap_or(defaults);

        Self {
            state: Mutex::new(State {
                palette,
                over: None,
                status: LightingMetricsDto::UNAVAILABLE,
            }),
            persistence,
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn publish(&self, status: LightingMetricsDto) -> bool {
        let mut state = self.lock();
        let changed = state.status != status;
        state.status = status;
        changed
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("lighting mutex poisoned")
    }
}

fn live(over: Option<StoredOverride>, now: Instant) -> Option<LightingOverride> {
    let stored = over?;

    let expires_in_s = match stored.expires_at {
        None => None,
        Some(at) if now >= at => return None,
        Some(at) => Some(at.saturating_duration_since(now).as_secs() as u32),
    };

    Some(LightingOverride {
        target: stored.target,
        expires_in_s,
    })
}

impl<P: LightingPersistence> LightingControl for LightingService<P> {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        let expires_at = duration_s.map(|s| Instant::now() + Duration::from_secs(u64::from(s)));
        self.lock().over = Some(StoredOverride { target, expires_at });
        self.notify.notify_one();
    }

    fn clear_override(&self) {
        self.lock().over = None;
        self.notify.notify_one();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        live(self.lock().over, Instant::now())
    }

    fn palette(&self) -> LightingPalette {
        self.lock().palette
    }

    fn apply_palette(
        &self,
        patch: LightingPalettePatch,
    ) -> Result<LightingPalette, LightingPaletteError> {
        let candidate = {
            let mut state = self.lock();
            let candidate = patch.merge_onto(state.palette);

            candidate
                .validate()
                .map_err(LightingPaletteError::Invalid)?;
            self.persistence
                .store(&candidate)
                .map_err(LightingPaletteError::NotPersisted)?;

            state.palette = candidate;
            candidate
        };

        self.notify.notify_one();

        Ok(candidate)
    }
}

impl<P: LightingPersistence> LightingStatusSource for LightingService<P> {
    fn lighting_status(&self) -> LightingMetricsDto {
        self.lock().status
    }
}

impl<P: LightingPersistence> LightingStatusSource for Arc<LightingService<P>> {
    fn lighting_status(&self) -> LightingMetricsDto {
        self.as_ref().lighting_status()
    }
}
