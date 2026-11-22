use tokio::time::Instant;

use super::service::PumpService;
use super::state::{Activity, FinishWrite, PendingFinish, State};
use crate::domain::pump::{
    litres, LevelConditions, Operation, OperationEvent, OperationKind, OperationPlan,
    OperationResult, SkipReason, SkippedWindow,
};
use crate::ports::inbound::StartError;
use crate::ports::outbound::{Clock, PumpStore};

const fn kind_slot(kind: OperationKind) -> usize {
    match kind {
        OperationKind::TopOff => 0,
        OperationKind::WaterChange => 1,
    }
}

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub fn fire_due_windows(&self, levels: Option<LevelConditions>) -> bool {
        let started = self.fire_due_windows_deferred(levels, Instant::now());
        self.persist_pending();
        started
    }

    pub(super) fn fire_due_windows_deferred(
        &self,
        levels: Option<LevelConditions>,
        now: Instant,
    ) -> bool {
        let (Some(epoch), Some(civil)) =
            (self.clock.epoch_seconds(), self.clock.local_civil_time())
        else {
            return false;
        };

        let minute = epoch / 60;
        let mut state = self.lock();

        if state.last_fired_minute == Some(minute) {
            return false;
        }
        state.last_fired_minute = Some(minute);
        state.levels = levels;

        let mut due: Vec<(OperationKind, usize, OperationPlan)> = Vec::new();

        for (index, window) in state.schedule.water_change.iter_mut().enumerate() {
            if window.due(civil, epoch) {
                due.push((
                    OperationKind::WaterChange,
                    index,
                    OperationPlan::water_change(window.plan),
                ));
            }
        }

        for (index, window) in state.schedule.top_off.iter_mut().enumerate() {
            if window.due(civil, epoch) {
                due.push((
                    OperationKind::TopOff,
                    index,
                    OperationPlan::top_off(window.plan),
                ));
            }
        }

        due.retain(|(kind, _, _)| {
            state.started_before_boot[kind_slot(*kind)].is_none_or(|started| started / 60 != minute)
        });

        let mut started = false;

        for (kind, index, plan) in due {
            match self.begin(&mut state, plan, now, Some(epoch)) {
                Ok(()) => started = true,
                Err(error) => {
                    let (reason, blocked_by) = match error {
                        StartError::Blocked(blocker) => (SkipReason::Blocked, Some(blocker)),
                        StartError::NotPersisted(_) => (SkipReason::Storage, None),
                        _ => (SkipReason::Busy, None),
                    };

                    tracing::warn!(
                        ?kind,
                        index,
                        ?reason,
                        ?blocked_by,
                        "scheduled pump window skipped"
                    );

                    state.last_skipped = Some(SkippedWindow {
                        kind,
                        index: index as u8,
                        reason,
                        blocked_by,
                        at: Some(epoch),
                    });
                }
            }
        }

        started
    }

    pub(super) fn start(&self, plan: OperationPlan) -> Result<(), StartError> {
        let epoch = self.clock.epoch_seconds();
        {
            let mut state = self.lock();

            if plan.kind == OperationKind::WaterChange
                && !state.settings.drain_fits_reservoir(plan.drain_s)
            {
                return Err(StartError::Invalid(
                    "the planned drain exceeds the water the reservoir holds",
                ));
            }

            self.begin(&mut state, plan, Instant::now(), epoch)?;
            self.flush_pending(&mut state);
        }
        self.notify.notify_one();
        Ok(())
    }

    fn begin(
        &self,
        state: &mut State,
        plan: OperationPlan,
        now: Instant,
        epoch: Option<u64>,
    ) -> Result<(), StartError> {
        state.admit(&plan, now, epoch)?;

        let operation = Operation::start(plan, epoch);
        if let Err(reason) = self.record_ahead(state, &operation, epoch) {
            tracing::error!(kind = ?plan.kind, reason, "pump operation not started, its checkpoint could not be stored");
            return Err(StartError::NotPersisted(reason));
        }

        if let Activity::Running(preempted) = std::mem::replace(&mut state.activity, Activity::Idle)
        {
            self.finish(state, &preempted, OperationResult::Aborted, epoch, now);
            state.pending_finish = None;
            state.pending.last = true;
        }

        state.activity = Activity::Running(operation);
        state.fresh_operation = true;
        tracing::info!(kind = ?plan.kind, "pump operation started");

        Ok(())
    }

    pub(super) fn finish(
        &self,
        state: &mut State,
        operation: &Operation,
        result: OperationResult,
        epoch: Option<u64>,
        now: Instant,
    ) {
        let sump_low = state.levels.is_some_and(|levels| levels.sump_low);
        let mut record = operation
            .record(result)
            .graded(state.settings.flow_lph, sump_low);
        record.finished_at = epoch;
        let result = record.result;

        state.last.remember(operation, record);

        if record.kind == OperationKind::WaterChange {
            state.water_change_ended = Some((now, result));
        }

        state.activity = Activity::Idle;
        state.pending.last = false;
        state.pending_finish = Some(PendingFinish {
            kind: record.kind,
            next: FinishWrite::MarkFinished,
            reported: false,
        });

        tracing::info!(kind = ?record.kind, ?result, "pump operation finished");

        let flows = state.settings.flow_lph;
        state.pending.events.push(OperationEvent {
            record,
            finished_at: epoch,
            drained_l: litres(record.drain_ran_s, flows.drain),
            refilled_l: litres(record.refill_ran_s, flows.makeup),
        });
    }
}

#[cfg(test)]
mod gap_tests;
#[cfg(test)]
mod tests;
