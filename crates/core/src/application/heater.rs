use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::time::Instant;

use crate::domain::heater::{
    decide, validate_override, HeaterFault, HeaterId, HeaterMode, HeaterSettings,
    HeaterSettingsPatch, HeaterStatus, HeatersStatus, OverrideState,
};
use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::{HeaterControl, HeaterSettingsError};
use crate::ports::outbound::{HeaterOutputs, HeaterPersistence, HeaterStatusSource};

#[derive(Debug, Clone, Copy)]
struct StoredOverride {
    state: OverrideState,
    expires_at: Option<Instant>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Output {
    on: bool,
    fault: Option<HeaterFault>,
}

struct State {
    settings: HeaterSettings,
    overrides: [Option<StoredOverride>; 2],
    outputs: [Option<Output>; 2],
}

impl State {
    fn live_override(&mut self, id: HeaterId, now: Instant) -> Option<StoredOverride> {
        let slot = &mut self.overrides[id.index()];

        if slot.is_some_and(|over| over.expires_at.is_some_and(|at| now >= at)) {
            *slot = None;
        }

        *slot
    }

    fn status(&mut self, id: HeaterId, now: Instant) -> HeaterStatus {
        let over = self.live_override(id, now);
        let output = self.outputs[id.index()];

        HeaterStatus {
            active: output.is_some_and(|output| output.on),
            mode: HeaterMode::from_override(over.map(|over| over.state)),
            fault: match output {
                Some(output) => output.fault,
                None => Some(HeaterFault::Unavailable),
            },
            override_expires_in_s: over
                .and_then(|over| over.expires_at)
                .map(|at| at.saturating_duration_since(now).as_millis().div_ceil(1000) as u32),
            target_celsius: self.settings.zone(id).target_celsius,
        }
    }

    fn all(&mut self, now: Instant) -> HeatersStatus {
        HeatersStatus {
            tank: self.status(HeaterId::Tank, now),
            sump: self.status(HeaterId::Sump, now),
        }
    }
}

pub struct HeaterService<P> {
    state: Mutex<State>,
    persistence: P,
    notify: Arc<Notify>,
}

impl<P: HeaterPersistence> HeaterService<P> {
    pub fn new(defaults: HeaterSettings, persistence: P) -> Self {
        let settings = persistence
            .load()
            .filter(|stored| stored.validate().is_ok())
            .unwrap_or(defaults);

        Self {
            state: Mutex::new(State {
                settings,
                overrides: [None; 2],
                outputs: [None; 2],
            }),
            persistence,
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    pub fn active_override(&self, id: HeaterId) -> Option<OverrideState> {
        self.lock()
            .live_override(id, Instant::now())
            .map(|over| over.state)
    }

    pub fn is_on(&self, id: HeaterId) -> bool {
        self.lock().outputs[id.index()].is_some_and(|output| output.on)
    }

    pub fn record(&self, id: HeaterId, on: bool, fault: Option<HeaterFault>) -> bool {
        let mut state = self.lock();
        let next = Output { on, fault };
        let previous = state.outputs[id.index()].replace(next);

        previous != Some(next)
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("heater mutex poisoned")
    }
}

impl<P: HeaterPersistence> HeaterControl for HeaterService<P> {
    fn status(&self) -> HeatersStatus {
        self.lock().all(Instant::now())
    }

    fn settings(&self) -> HeaterSettings {
        self.lock().settings
    }

    fn apply_settings(
        &self,
        patch: HeaterSettingsPatch,
    ) -> Result<HeaterSettings, HeaterSettingsError> {
        let candidate = {
            let mut state = self.lock();
            let candidate = patch.merge_onto(state.settings);

            candidate.validate().map_err(HeaterSettingsError::Invalid)?;
            self.persistence
                .store(&candidate)
                .map_err(HeaterSettingsError::NotPersisted)?;

            state.settings = candidate;
            candidate
        };

        self.notify.notify_one();

        Ok(candidate)
    }

    fn set_override(
        &self,
        id: HeaterId,
        state: OverrideState,
        duration_s: Option<u32>,
    ) -> Result<HeaterStatus, &'static str> {
        validate_override(state, duration_s)?;

        let now = Instant::now();
        let status = {
            let mut guard = self.lock();
            guard.overrides[id.index()] = Some(StoredOverride {
                state,
                expires_at: duration_s.map(|seconds| now + Duration::from_secs(seconds.into())),
            });
            guard.status(id, now)
        };

        self.notify.notify_one();

        Ok(status)
    }

    fn clear_override(&self, id: HeaterId) {
        self.lock().overrides[id.index()] = None;
        self.notify.notify_one();
    }
}

impl<P: HeaterPersistence> HeaterStatusSource for HeaterService<P> {
    fn heater_status(&self) -> HeatersStatus {
        self.status()
    }
}

impl<P: HeaterPersistence> HeaterControl for Arc<HeaterService<P>> {
    fn status(&self) -> HeatersStatus {
        self.as_ref().status()
    }

    fn settings(&self) -> HeaterSettings {
        self.as_ref().settings()
    }

    fn apply_settings(
        &self,
        patch: HeaterSettingsPatch,
    ) -> Result<HeaterSettings, HeaterSettingsError> {
        self.as_ref().apply_settings(patch)
    }

    fn set_override(
        &self,
        id: HeaterId,
        state: OverrideState,
        duration_s: Option<u32>,
    ) -> Result<HeaterStatus, &'static str> {
        self.as_ref().set_override(id, state, duration_s)
    }

    fn clear_override(&self, id: HeaterId) {
        self.as_ref().clear_override(id);
    }
}

impl<P: HeaterPersistence> HeaterStatusSource for Arc<HeaterService<P>> {
    fn heater_status(&self) -> HeatersStatus {
        self.as_ref().status()
    }
}

pub struct HeaterController<O, P> {
    outputs: O,
    service: Arc<HeaterService<P>>,
}

impl<O: HeaterOutputs, P: HeaterPersistence> HeaterController<O, P> {
    pub fn new(outputs: O, service: Arc<HeaterService<P>>) -> Self {
        Self { outputs, service }
    }

    pub fn tick(&mut self, snapshot: Option<&TankTelemetryDto>) -> bool {
        let settings = self.service.settings();
        let mut changed = false;

        for id in HeaterId::ALL {
            let decision = decide(
                snapshot.and_then(|snapshot| reading(snapshot, id)),
                settings.zone(id),
                settings.cutoff_celsius,
                self.service.active_override(id),
                self.service.is_on(id),
            );

            let on = self.drive(id, decision.on);
            changed |= self.service.record(id, on, decision.fault);
        }

        changed
    }

    fn drive(&mut self, id: HeaterId, on: bool) -> bool {
        match self.outputs.set(id, on) {
            Ok(()) => on,
            Err(reason) => {
                tracing::error!(heater = id.name(), reason, "heater output failed");

                if on {
                    if let Err(reason) = self.outputs.set(id, false) {
                        tracing::error!(
                            heater = id.name(),
                            reason,
                            "heater output could not be forced off"
                        );
                    }
                }

                false
            }
        }
    }
}

fn reading(snapshot: &TankTelemetryDto, id: HeaterId) -> Option<f32> {
    match id {
        HeaterId::Tank => snapshot.temperature.tank_celsius,
        HeaterId::Sump => snapshot.temperature.sump_celsius,
    }
}

pub async fn heater_loop<O, P>(
    mut controller: HeaterController<O, P>,
    mut snapshots: watch::Receiver<TankTelemetryDto>,
    api_changed: Arc<Notify>,
    republish: Arc<Notify>,
    watchdog: Duration,
) where
    O: HeaterOutputs,
    P: HeaterPersistence,
{
    let mut latest: Option<TankTelemetryDto> = None;
    let mut deadline = Instant::now() + watchdog;

    loop {
        let api_wake = tokio::select! {
            received = snapshots.changed() => {
                if received.is_err() {
                    controller.tick(None);
                    return;
                }

                latest = Some(snapshots.borrow_and_update().clone());
                deadline = Instant::now() + watchdog;
                false
            }
            () = api_changed.notified() => true,
            () = tokio::time::sleep_until(deadline) => {
                latest = None;
                deadline = Instant::now() + watchdog;
                false
            }
        };

        if Instant::now() >= deadline {
            latest = None;
        }

        if controller.tick(latest.as_ref()) || api_wake {
            republish.notify_one();
        }
    }
}

#[cfg(test)]
mod tests;
