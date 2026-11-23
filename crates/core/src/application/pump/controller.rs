use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::time::{Instant, MissedTickBehavior};

use super::service::PumpService;
use super::watches::{CirculationWatch, ReturnLockout};
use crate::domain::pump::{
    decide, Blocker, ChannelDecision, ChannelFault, DecisionInput, LevelConditions, PumpId,
};
use crate::domain::water::{HarnessWatch, WaterLevelSnapshot};
use crate::ports::outbound::{Clock, LevelSample, PumpOutputs, PumpStore};

#[derive(Debug, Clone, Copy)]
pub struct PumpTiming {
    pub return_restart_delay: Duration,
    pub level_stale: Duration,
    pub tick: Duration,
    pub checkpoint_interval: Duration,
    pub runtime_flush: Duration,
}

impl PumpTiming {
    pub const DEFAULT: Self = Self {
        return_restart_delay: Duration::from_secs(30),
        level_stale: Duration::from_secs(1),
        tick: Duration::from_secs(1),
        checkpoint_interval: Duration::from_secs(3),
        runtime_flush: Duration::from_secs(600),
    };
}

pub struct PumpController<O, S, C> {
    outputs: O,
    service: Arc<PumpService<S, C>>,
    timing: PumpTiming,
    lockout: ReturnLockout,
    circulation: CirculationWatch,
    harness: HarnessWatch,
    last_tick: Option<Instant>,
    last_checkpoint: Option<Instant>,
    last_flush: Option<Instant>,
    possibly_on: [bool; 4],
}

const fn opposite_mover(id: PumpId) -> Option<PumpId> {
    match id {
        PumpId::Makeup => Some(PumpId::Drain),
        PumpId::Drain => Some(PumpId::Makeup),
        PumpId::Return | PumpId::Uv => None,
    }
}

impl<O: PumpOutputs, S: PumpStore, C: Clock> PumpController<O, S, C> {
    pub fn new(outputs: O, service: Arc<PumpService<S, C>>, timing: PumpTiming) -> Self {
        Self {
            outputs,
            service,
            timing,
            lockout: ReturnLockout::default(),
            circulation: CirculationWatch::default(),
            harness: HarnessWatch::default(),
            last_tick: None,
            last_checkpoint: None,
            last_flush: None,
            possibly_on: PumpId::ALL.map(PumpId::fail_safe_on),
        }
    }

    pub fn tick(&mut self, snapshot: Option<&WaterLevelSnapshot>, now: Instant) -> bool {
        let elapsed = self
            .last_tick
            .map_or(Duration::ZERO, |last| now.saturating_duration_since(last));
        self.last_tick = Some(now);

        let suspect = self.harness.update(snapshot, elapsed);
        let suspect_changed = self.service.report_level_sensors(suspect);
        let refill_stop = self.service.refill_stop_sensor();
        let levels = snapshot
            .filter(|_| !suspect)
            .map(|snapshot| LevelConditions::from_snapshot(snapshot, refill_stop));

        let started = self.service.fire_due_windows_deferred(levels, now);
        let observed = self.service.observe_deferred(levels, elapsed, now);
        let locked_out = self
            .lockout
            .update(levels, now, self.timing.return_restart_delay);
        let starved = self.circulation.update(
            levels.map(|levels| levels.sump_low),
            observed.phase.is_some(),
            now,
        );
        let starvation_changed = self.service.report_circulation(starved);
        let return_held_after_drain = self.service.hold_return_after_drain(levels.is_none(), now);

        let mut decisions = decide(DecisionInput {
            flood_locked: observed.flood_locked,
            levels,
            return_locked_out: locked_out,
            return_forced_off: observed.return_forced_off,
            uv_forced_off: observed.uv_forced_off,
            phase: observed.phase,
            return_held_after_drain,
            supervision_unavailable: observed.supervision_unavailable,
            drain_unrecorded: observed.drain_unrecorded,
        });

        let mut driven: [(bool, Option<ChannelFault>); 4] = [(false, None); 4];
        for id in PumpId::ALL {
            if !decisions[id.index()].on {
                driven[id.index()] = self.drive(id, false);
            }
        }

        for id in PumpId::ALL {
            let decision = &mut decisions[id.index()];
            if !decision.on {
                continue;
            }

            let return_failed = id == PumpId::Uv && driven[PumpId::Return.index()].1.is_some();
            let still_running = |other: PumpId| {
                let (on, fault) = driven[other.index()];
                on || fault.is_some()
            };
            let interlocked = opposite_mover(id).is_some_and(still_running)
                || (id == PumpId::Drain && still_running(PumpId::Return));

            if return_failed || interlocked {
                *decision = ChannelDecision {
                    on: false,
                    blocked_by: Some(if return_failed {
                        Blocker::ReturnOff
                    } else {
                        Blocker::Interlock
                    }),
                };
                driven[id.index()] = self.drive(id, false);
            } else {
                driven[id.index()] = self.drive(id, true);
            }
        }

        let mut outputs_changed = false;
        let mut on_changed = false;
        for id in PumpId::ALL {
            let (on, fault) = driven[id.index()];
            let outcome = self
                .service
                .record(id, on, fault, decisions[id.index()].blocked_by);
            outputs_changed |= outcome.changed;
            on_changed |= outcome.on_changed;
        }

        self.service.persist_pending();

        let last_flush = *self.last_flush.get_or_insert(now);
        if on_changed || now.saturating_duration_since(last_flush) >= self.timing.runtime_flush {
            self.service.flush_runtime();
            self.last_flush = Some(now);
        }

        let last_checkpoint = *self.last_checkpoint.get_or_insert(now);
        if now.saturating_duration_since(last_checkpoint) >= self.timing.checkpoint_interval {
            let durable = self.service.checkpoint();
            self.last_checkpoint = Some(now);

            if !durable && self.possibly_on[PumpId::Drain.index()] {
                let (on, fault) = self.drive(PumpId::Drain, false);
                outputs_changed |= self
                    .service
                    .record(PumpId::Drain, on, fault, Some(Blocker::StorageUnavailable))
                    .changed;
            }
        }

        started || observed.changed || outputs_changed || starvation_changed || suspect_changed
    }

    /// Supervise the relay module while normal pump automation is disabled.
    pub fn tick_flood_only(&mut self, latched: bool, released_on: [bool; 4]) -> bool {
        let mut changed = false;
        for id in PumpId::ALL {
            let (on, fault) = self.drive(id, !latched && released_on[id.index()]);
            changed |= self
                .service
                .record(id, on, fault, latched.then_some(Blocker::Leak))
                .changed;
        }
        changed
    }

    fn drive(&mut self, id: PumpId, on: bool) -> (bool, Option<ChannelFault>) {
        match self.outputs.set(id, on) {
            Ok(()) => {
                self.possibly_on[id.index()] = on;
                (on, None)
            }
            Err(reason) => {
                tracing::error!(pump = id.name(), on, reason, "pump output failed");
                (
                    self.possibly_on[id.index()],
                    Some(ChannelFault::OutputError),
                )
            }
        }
    }
}

pub async fn pump_loop<O, S, C>(
    mut controller: PumpController<O, S, C>,
    mut levels: watch::Receiver<Option<LevelSample>>,
    api_changed: Arc<Notify>,
    republish: Arc<Notify>,
    timing: PumpTiming,
    heartbeat: Arc<AtomicU32>,
) where
    O: PumpOutputs,
    S: PumpStore,
    C: Clock,
{
    let mut latest: Option<LevelSample> = None;
    let mut closed = false;
    let mut ticker = tokio::time::interval(timing.tick);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        heartbeat.fetch_add(1, Ordering::Relaxed);

        let api_wake = tokio::select! {
            received = levels.changed(), if !closed => {
                match received {
                    Ok(()) => latest = levels.borrow_and_update().clone(),
                    Err(_) => {
                        closed = true;
                        latest = None;
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
            .filter(|sample| now.saturating_duration_since(sample.measured_at) < timing.level_stale)
            .map(|sample| &sample.levels);

        if controller.tick(fresh, now) || api_wake {
            republish.notify_one();
        }
    }
}

#[cfg(test)]
mod interlock_tests;
#[cfg(test)]
mod tests;
