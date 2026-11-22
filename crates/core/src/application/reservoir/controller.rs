use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::time::{Instant, MissedTickBehavior};

use super::observe::Observation;
use super::service::ReservoirService;
use super::timing::ReservoirTiming;
use crate::domain::reservoir::{
    decide, ChannelDecision, ReservoirBlocker, ReservoirChannel, ReservoirConditions,
};
use crate::domain::water::{HarnessWatch, WaterLevelSnapshot};
use crate::ports::outbound::{
    Clock, LeakSource, LevelSample, ReservoirOutputs, ReservoirStore, TdsSample,
    WaterChangeActivity,
};

const OFF_ORDER: [ReservoirChannel; 4] = [
    ReservoirChannel::Booster,
    ReservoirChannel::RoValve,
    ReservoirChannel::TapValve,
    ReservoirChannel::Mixer,
];
const ON_ORDER: [ReservoirChannel; 4] = [
    ReservoirChannel::RoValve,
    ReservoirChannel::Booster,
    ReservoirChannel::TapValve,
    ReservoirChannel::Mixer,
];
const WATER_PATH: [ReservoirChannel; 3] = [
    ReservoirChannel::RoValve,
    ReservoirChannel::Booster,
    ReservoirChannel::TapValve,
];

pub struct ReservoirController<O, S, C> {
    outputs: O,
    service: Arc<ReservoirService<S, C>>,
    timing: ReservoirTiming,
    harness: HarnessWatch,
    possibly_on: [bool; 4],
    off_failed: [bool; 4],
    on_failed: bool,
    ro_open_since: Option<Instant>,
    last_tick: Option<Instant>,
    last_flush: Option<Instant>,
}

impl<O: ReservoirOutputs, S: ReservoirStore, C: Clock> ReservoirController<O, S, C> {
    pub fn new(outputs: O, service: Arc<ReservoirService<S, C>>, timing: ReservoirTiming) -> Self {
        Self {
            outputs,
            service,
            timing,
            harness: HarnessWatch::default(),
            possibly_on: [false; 4],
            off_failed: [false; 4],
            on_failed: false,
            ro_open_since: None,
            last_tick: None,
            last_flush: None,
        }
    }

    pub fn tick(
        &mut self,
        snapshot: Option<&WaterLevelSnapshot>,
        tds: Option<TdsSample>,
        leak: Option<bool>,
        water_change_active: bool,
        now: Instant,
    ) -> bool {
        let elapsed = self
            .last_tick
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_tick = Some(now);

        let suspect = self.harness.update(snapshot, elapsed);
        let levels = snapshot.filter(|_| !suspect);
        let plan = self.service.observe(Observation {
            conditions: levels.map(|levels| ReservoirConditions::from_snapshot(levels, leak)),
            leak,
            water_change_active,
            tds,
            ran: self.possibly_on,
            uncertain: WATER_PATH
                .iter()
                .any(|channel| self.off_failed[channel.index()]),
            on_failed: self.on_failed,
            ro_valve_open_ms: self.ro_open_since.map_or(0, |since| {
                now.saturating_duration_since(since).as_millis() as u64
            }),
            elapsed,
            now,
        });

        let mut decisions = decide(plan.input);
        let mut driven = [(false, false); 4];
        for channel in OFF_ORDER {
            if !decisions[channel.index()].on {
                driven[channel.index()] = self.drive(channel, false);
            }
        }

        for channel in ON_ORDER {
            let index = channel.index();
            if !decisions[index].on {
                continue;
            }
            let open = |other: ReservoirChannel| {
                let (on, error) = driven[other.index()];
                on || error
            };
            let interlocked = match channel {
                ReservoirChannel::RoValve => open(ReservoirChannel::TapValve),
                ReservoirChannel::Booster => {
                    driven[ReservoirChannel::RoValve.index()] != (true, false)
                }
                ReservoirChannel::TapValve => {
                    open(ReservoirChannel::RoValve) || open(ReservoirChannel::Booster)
                }
                ReservoirChannel::Mixer => false,
            };
            if interlocked {
                decisions[index] = ChannelDecision::blocked(ReservoirBlocker::Interlock);
                driven[index] = self.drive(channel, false);
            } else {
                driven[index] = self.drive(channel, true);
            }
        }

        self.on_failed = WATER_PATH
            .iter()
            .any(|channel| decisions[channel.index()].on && driven[channel.index()].1);
        self.ro_open_since = match driven[ReservoirChannel::RoValve.index()] {
            (true, false) => Some(self.ro_open_since.unwrap_or(now)),
            _ => None,
        };

        let mut changed = plan.changed;
        let mut on_changed = false;
        for channel in ReservoirChannel::ALL {
            let (on, error) = driven[channel.index()];
            let blocked_by = decisions[channel.index()].blocked_by;
            let recorded = self.service.record(channel, on, error, blocked_by);
            changed |= recorded.changed;
            on_changed |= recorded.on_changed;
        }
        self.service.persist_pending();

        let last_flush = *self.last_flush.get_or_insert(now);
        if on_changed || now.saturating_duration_since(last_flush) >= self.timing.runtime_flush {
            self.service.flush_runtime();
            self.last_flush = Some(now);
        }

        changed
    }

    fn drive(&mut self, channel: ReservoirChannel, on: bool) -> (bool, bool) {
        let index = channel.index();
        match self.outputs.set(channel, on) {
            Ok(()) => {
                self.possibly_on[index] = on;
                self.off_failed[index] = false;
                (on, false)
            }
            Err(reason) => {
                tracing::error!(channel = channel.name(), on, reason, "barrel output failed");
                if !on {
                    self.off_failed[index] = true;
                }
                (self.possibly_on[index], true)
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn reservoir_loop<O, S, C, L, W>(
    mut controller: ReservoirController<O, S, C>,
    mut levels: watch::Receiver<Option<LevelSample>>,
    mut tds: watch::Receiver<Option<TdsSample>>,
    leak: L,
    water_change: W,
    api_changed: Arc<Notify>,
    republish: Arc<Notify>,
    timing: ReservoirTiming,
    heartbeat: Arc<AtomicU32>,
) where
    O: ReservoirOutputs,
    S: ReservoirStore,
    C: Clock,
    L: LeakSource,
    W: WaterChangeActivity,
{
    let mut latest: Option<LevelSample> = None;
    let mut sample: Option<TdsSample> = None;
    let mut levels_closed = false;
    let mut tds_closed = false;
    let mut ticker = tokio::time::interval(timing.tick);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        heartbeat.fetch_add(1, Ordering::Relaxed);

        let api_wake = tokio::select! {
            received = levels.changed(), if !levels_closed => {
                match received {
                    Ok(()) => latest = levels.borrow_and_update().clone(),
                    Err(_) => {
                        levels_closed = true;
                        latest = None;
                    }
                }
                false
            }
            received = tds.changed(), if !tds_closed => {
                match received {
                    Ok(()) => sample = *tds.borrow_and_update(),
                    Err(_) => {
                        tds_closed = true;
                        sample = None;
                    }
                }
                false
            }
            () = api_changed.notified() => true,
            _ = ticker.tick() => false,
        };

        let now = Instant::now();
        let fresh = latest
            .as_ref()
            .filter(|level| now.saturating_duration_since(level.measured_at) < timing.level_stale)
            .map(|level| &level.levels);

        if controller.tick(
            fresh,
            sample,
            leak.leak(),
            water_change.water_change_active(),
            now,
        ) || api_wake
        {
            republish.notify_one();
        }
    }
}

#[cfg(test)]
mod loop_tests;
#[cfg(test)]
mod tests;
