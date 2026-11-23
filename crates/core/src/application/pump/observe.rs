use std::time::Duration;

use tokio::time::Instant;

use super::service::PumpService;
use super::state::{Activity, Output};
use crate::domain::pump::{
    resume_verdict, Blocker, ChannelFault, LevelConditions, Phase, PumpId, ResumeVerdict, Step,
    Supervision,
};
use crate::ports::outbound::{Clock, PumpStore};

#[derive(Debug, Clone, Copy, Default)]
pub struct RecordOutcome {
    pub changed: bool,
    pub on_changed: bool,
}

pub struct Observation {
    pub(super) phase: Option<Phase>,
    pub(super) return_forced_off: bool,
    pub(super) uv_forced_off: bool,
    pub(super) supervision_unavailable: bool,
    pub(super) drain_unrecorded: bool,
    pub(super) changed: bool,
}

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub fn observe(
        &self,
        levels: Option<LevelConditions>,
        elapsed: Duration,
        now: Instant,
    ) -> Observation {
        let observation = self.observe_deferred(levels, elapsed, now);
        self.persist_pending();
        observation
    }

    pub(super) fn observe_deferred(
        &self,
        levels: Option<LevelConditions>,
        elapsed: Duration,
        now: Instant,
    ) -> Observation {
        let epoch = self.clock.epoch_seconds();
        let mut state = self.lock();
        state.levels = levels;

        let elapsed_ms = elapsed.as_millis() as u64;
        let mut wall_elapsed_ms = if std::mem::take(&mut state.fresh_operation) {
            0
        } else {
            elapsed_ms
        };
        let mut operation_elapsed_ms = wall_elapsed_ms;

        if let Activity::Running(operation) = &state.activity {
            let channel_ran = |id: PumpId| state.outputs[id.index()].is_some_and(|o| o.on);

            let idle_channel = match operation.phase {
                Phase::Draining => !channel_ran(PumpId::Drain),
                Phase::Refilling => !channel_ran(PumpId::Makeup),
                Phase::Settling => false,
            };

            if idle_channel {
                operation_elapsed_ms = 0;
            }
        }

        for id in PumpId::ALL {
            let credited_ms = match state.reset_instant[id.index()].take() {
                Some(reset) => {
                    elapsed_ms.min(now.saturating_duration_since(reset).as_millis() as u64)
                }
                None => elapsed_ms,
            };

            if state.outputs[id.index()].is_some_and(|output| output.on) {
                state.runtime_ms[id.index()] += credited_ms;
            }
        }

        let mut changed = false;

        if let Activity::Resuming(checkpoint) = &state.activity {
            let checkpoint = checkpoint.clone();

            if state.resume_sync_epoch.is_none() {
                state.resume_sync_epoch = epoch;
            }

            let ready = checkpoint.operation.ready_to_resume(levels).is_ok()
                && state.supervision != Supervision::Unavailable;
            let verdict = if checkpoint.operation.resumed {
                ResumeVerdict::Suspend
            } else {
                resume_verdict(
                    checkpoint.checkpoint_at,
                    state.resume_sync_epoch,
                    self.clock.uptime_seconds(),
                    state.settings.resume_window_s,
                    ready,
                )
            };

            match verdict {
                ResumeVerdict::Wait => {}
                ResumeVerdict::Resume => {
                    let mut operation = checkpoint.operation.clone();
                    operation.resume();

                    match self.record_ahead(&mut state, &operation, epoch) {
                        Ok(()) => {
                            tracing::info!(
                                kind = ?operation.plan.kind,
                                phase = ?operation.phase,
                                "pump operation resumed after a restart"
                            );
                            state.activity = Activity::Running(operation);
                            operation_elapsed_ms = 0;
                            wall_elapsed_ms = 0;
                        }
                        Err(reason) => {
                            tracing::error!(
                                kind = ?operation.plan.kind,
                                reason,
                                "pump operation not resumed, its checkpoint could not be stored"
                            );
                            state.activity = Activity::Suspended(checkpoint.operation);
                        }
                    }
                    changed = true;
                }
                ResumeVerdict::Suspend => {
                    tracing::warn!(
                        kind = ?checkpoint.operation.plan.kind,
                        "pump operation suspended after a restart"
                    );
                    state.activity = Activity::Suspended(checkpoint.operation);
                    changed = true;
                }
            }
        }

        let mut finished = None;
        let mut phase_changed = false;

        if let Activity::Running(operation) = &mut state.activity {
            match operation.advance(operation_elapsed_ms, wall_elapsed_ms, levels) {
                Step::Continue => {}
                Step::PhaseChanged => {
                    tracing::info!(phase = ?operation.phase, "pump operation phase");
                    phase_changed = true;
                }
                Step::Finished(result) => finished = Some((operation.clone(), result)),
            }
        }

        if phase_changed {
            state.pending.checkpoint = true;
            changed = true;
        }

        if let Some((operation, result)) = finished {
            self.finish(&mut state, &operation, result, epoch, now);
            changed = true;
        }

        let return_forced_off = state.forced_off(PumpId::Return, now, epoch);
        let uv_forced_off = state.forced_off(PumpId::Uv, now, epoch);

        if state.pending.overrides {
            changed = true;
        }

        Observation {
            phase: state.phase(),
            return_forced_off,
            uv_forced_off,
            supervision_unavailable: state.supervision == Supervision::Unavailable,
            drain_unrecorded: state.checkpoint_failed,
            changed,
        }
    }

    pub fn record(
        &self,
        id: PumpId,
        on: bool,
        fault: Option<ChannelFault>,
        blocked_by: Option<Blocker>,
    ) -> RecordOutcome {
        let next = Output {
            on,
            fault,
            blocked_by,
        };

        let mut state = self.lock();
        let previous = state.outputs[id.index()].replace(next);

        if id == PumpId::Drain && on && fault.is_none() {
            if let Activity::Running(operation) = &mut state.activity {
                if operation.phase == Phase::Draining && !operation.drain_switched {
                    operation.drain_switched = true;
                    state.pending.checkpoint = true;
                }
            }
        }

        RecordOutcome {
            changed: previous != Some(next),
            on_changed: previous.map(|output| output.on) != Some(on),
        }
    }
}

#[cfg(test)]
mod resume_tests;
#[cfg(test)]
mod tests;
