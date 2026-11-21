use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::time::{Instant, MissedTickBehavior};

use crate::domain::heater::{
    decide, validate_override, Decision, HeaterFault, HeaterId, HeaterMode, HeaterSettings,
    HeaterSettingsPatch, HeaterStatus, HeatersStatus, OverrideState,
};
use crate::ports::inbound::{HeaterControl, HeaterSettingsError};
use crate::ports::outbound::{
    HeaterOutputs, HeaterPersistence, HeaterStatusSource, TemperatureSample,
};

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

    pub fn next_override_expiry(&self) -> Option<Instant> {
        let now = Instant::now();
        let mut state = self.lock();

        HeaterId::ALL
            .into_iter()
            .filter_map(|id| state.live_override(id, now)?.expires_at)
            .min()
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
    levels: [bool; 2],
    uncertain: [bool; 2],
}

impl<O: HeaterOutputs, P: HeaterPersistence> HeaterController<O, P> {
    pub fn new(outputs: O, service: Arc<HeaterService<P>>) -> Self {
        Self {
            outputs,
            service,
            levels: [false; 2],
            uncertain: [false; 2],
        }
    }

    pub fn tick(&mut self, sample: Option<&TemperatureSample>) -> bool {
        let settings = self.service.settings();
        let mut changed = false;

        for id in HeaterId::ALL {
            let decision = decide(
                sample.and_then(|sample| reading(sample, id)),
                settings.zone(id),
                settings.cutoff_celsius,
                self.service.active_override(id),
                self.service.is_on(id),
            );

            let output = self.drive(id, decision);
            changed |= self.service.record(id, output.on, output.fault);
        }

        changed
    }

    fn retrying(&self) -> bool {
        self.uncertain.contains(&true)
    }

    fn drive(&mut self, id: HeaterId, decision: Decision) -> Output {
        let index = id.index();

        if self.uncertain[index] {
            if let Err(reason) = self.write(id, false) {
                tracing::error!(heater = id.name(), reason, "heater output still not off");
                return self.output_error(id);
            }

            self.uncertain[index] = false;
        }

        match self.write(id, decision.on) {
            Ok(()) => Output {
                on: decision.on,
                fault: decision.fault,
            },
            Err(reason) => {
                tracing::error!(heater = id.name(), reason, "heater output failed");

                let off = if decision.on {
                    self.write(id, false)
                } else {
                    Err(reason)
                };

                if let Err(reason) = off {
                    tracing::error!(
                        heater = id.name(),
                        reason,
                        "heater output could not be forced off"
                    );
                    self.uncertain[index] = true;
                }

                self.output_error(id)
            }
        }
    }

    fn write(&mut self, id: HeaterId, on: bool) -> Result<(), &'static str> {
        self.outputs.set(id, on)?;
        self.levels[id.index()] = on;
        Ok(())
    }

    fn output_error(&self, id: HeaterId) -> Output {
        Output {
            on: self.levels[id.index()],
            fault: Some(HeaterFault::OutputError),
        }
    }
}

fn reading(sample: &TemperatureSample, id: HeaterId) -> Option<f32> {
    match id {
        HeaterId::Tank => sample.tank_celsius,
        HeaterId::Sump => sample.sump_celsius,
    }
}

pub async fn probe_heaters(outputs: &mut impl HeaterOutputs, period: Duration) -> ! {
    let mut ticker = tokio::time::interval(period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        for id in HeaterId::ALL {
            ticker.tick().await;
            match outputs.set(id, true) {
                Ok(()) => tracing::info!(heater = id.name(), on = true, "SSR probe"),
                Err(reason) => tracing::error!(heater = id.name(), reason, "SSR probe ON failed"),
            }
            loop {
                ticker.tick().await;
                match outputs.set(id, false) {
                    Ok(()) => {
                        tracing::info!(heater = id.name(), on = false, "SSR probe");
                        break;
                    }
                    Err(reason) => {
                        tracing::error!(heater = id.name(), reason, "SSR probe retrying OFF")
                    }
                }
            }
        }
    }
}

const OUTPUT_RETRY: Duration = Duration::from_secs(1);

pub async fn heater_loop<O, P>(
    mut controller: HeaterController<O, P>,
    mut samples: watch::Receiver<Option<TemperatureSample>>,
    api_changed: Arc<Notify>,
    republish: Arc<Notify>,
    watchdog: Duration,
    heartbeat: Arc<AtomicU32>,
) where
    O: HeaterOutputs,
    P: HeaterPersistence,
{
    let mut latest: Option<TemperatureSample> = None;
    let mut stale_at = Some(Instant::now() + watchdog);
    let mut ticker = tokio::time::interval(OUTPUT_RETRY);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        heartbeat.fetch_add(1, Ordering::Relaxed);

        let wake_at = [stale_at, controller.service.next_override_expiry()]
            .into_iter()
            .flatten()
            .min();

        let announce = tokio::select! {
            biased;
            received = samples.changed() => {
                if received.is_err() {
                    controller.tick(None);
                    return;
                }

                latest = *samples.borrow_and_update();
                false
            }
            () = api_changed.notified() => true,
            () = sleep_until_some(wake_at) => true,
            _ = ticker.tick() => {
                if !controller.retrying() {
                    continue;
                }
                false
            }
        };

        let now = Instant::now();
        let fresh = latest.filter(|sample| now < sample.measured_at + watchdog);
        stale_at = fresh.map(|sample| sample.measured_at + watchdog);

        if controller.tick(fresh.as_ref()) || announce {
            republish.notify_one();
        }
    }
}

async fn sleep_until_some(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => core::future::pending().await,
    }
}

#[cfg(test)]
mod tests;
