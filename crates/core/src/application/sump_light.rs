use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

use super::sleep_until_some;
use crate::domain::sump_light::{validate_request, SumpLightStatus};
use crate::ports::inbound::SumpLightControl;
use crate::ports::outbound::{SumpLightOutputs, SumpLightStatusSource};

const RETRY_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
struct State {
    requested: bool,
    expires_at: Option<Instant>,
    applied: Option<bool>,
}

impl State {
    fn expire(&mut self, now: Instant) {
        if self.expires_at.is_some_and(|at| now >= at) {
            self.requested = false;
            self.expires_at = None;
        }
    }

    fn status(&self, now: Instant) -> SumpLightStatus {
        SumpLightStatus {
            on: self.applied.unwrap_or(false),
            expires_in_s: self
                .expires_at
                .map(|at| at.saturating_duration_since(now).as_millis().div_ceil(1000) as u32),
            available: self.applied.is_some(),
        }
    }
}

#[derive(Default)]
pub struct SumpLightService {
    state: Mutex<State>,
    notify: Arc<Notify>,
}

impl SumpLightService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    fn desired(&self) -> (bool, Option<Instant>) {
        let mut state = self.lock();
        state.expire(Instant::now());
        (state.requested, state.expires_at)
    }

    fn record(&self, on: bool) {
        self.lock().applied = Some(on);
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().expect("sump light mutex poisoned")
    }
}

impl SumpLightControl for SumpLightService {
    fn status(&self) -> SumpLightStatus {
        let now = Instant::now();
        let mut state = self.lock();
        state.expire(now);
        state.status(now)
    }

    fn set(&self, on: bool, duration_s: Option<u32>) -> Result<SumpLightStatus, &'static str> {
        validate_request(on, duration_s)?;

        let now = Instant::now();
        let status = {
            let mut state = self.lock();
            state.requested = on;
            state.expires_at = duration_s.map(|s| now + Duration::from_secs(u64::from(s)));
            state.status(now)
        };

        self.notify.notify_one();

        Ok(status)
    }
}

impl SumpLightStatusSource for SumpLightService {
    fn sump_light_status(&self) -> SumpLightStatus {
        self.status()
    }
}

impl SumpLightControl for Arc<SumpLightService> {
    fn status(&self) -> SumpLightStatus {
        self.as_ref().status()
    }

    fn set(&self, on: bool, duration_s: Option<u32>) -> Result<SumpLightStatus, &'static str> {
        self.as_ref().set(on, duration_s)
    }
}

impl SumpLightStatusSource for Arc<SumpLightService> {
    fn sump_light_status(&self) -> SumpLightStatus {
        self.as_ref().status()
    }
}

pub async fn sump_light_loop<O: SumpLightOutputs>(
    service: Arc<SumpLightService>,
    mut outputs: O,
    republish: Arc<Notify>,
) {
    let changes = service.change_signal();
    let mut applied: Option<bool> = None;
    let mut published: Option<(bool, Option<Instant>)> = None;

    loop {
        let (want, expires_at) = service.desired();

        let retry_at = if applied == Some(want) {
            None
        } else {
            match outputs.set(want) {
                Ok(()) => {
                    applied = Some(want);
                    service.record(want);
                    None
                }
                Err(error) => {
                    tracing::warn!(%error, on = want, "sump light output failed");
                    Some(Instant::now() + RETRY_INTERVAL)
                }
            }
        };

        if applied == Some(want) && published != Some((want, expires_at)) {
            published = Some((want, expires_at));
            republish.notify_one();
        }

        let wake_at = [expires_at, retry_at].into_iter().flatten().min();

        tokio::select! {
            biased;
            () = changes.notified() => {}
            () = sleep_until_some(wake_at) => {}
        }
    }
}

#[cfg(test)]
mod tests;
