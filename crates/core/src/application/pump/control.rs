use std::time::Duration;

use tokio::time::Instant;

use super::schedule::Schedule;
use super::service::PumpService;
use super::state::{Activity, Expiry};
use crate::domain::flood::FloodStatus;
use crate::domain::pump::{
    validate_override, validate_top_off, Blocker, ChannelStatus, OperationKind, OperationPlan,
    OperationRecord, OperationResult, OverrideError, PumpId, PumpSettings, PumpSettingsPatch,
    PumpsStatus, PumpsTelemetry, Supervision, WaterChangePlan,
};
use crate::ports::inbound::{
    CancelError, FloodResetError, PumpControl, PumpSettingsError, StartError,
};
use crate::ports::outbound::{Clock, PumpRecord, PumpStatusSource, PumpStore, WaterChangeActivity};

impl<S: PumpStore, C: Clock> PumpControl for PumpService<S, C> {
    fn status(&self) -> PumpsStatus {
        let now = Instant::now();
        let epoch = self.clock.epoch_seconds();
        let mut state = self.lock();

        PumpsStatus {
            channels: state.channels(now, epoch),
            operation: state.operation_status(),
            last_top_off: state.last.top_off,
            last_water_change: state.last.water_change,
            last_skipped: state.last_skipped,
            circulation_starved: state.circulation_starved,
            level_sensors_suspect: state.level_sensors_suspect,
            return_held_after_drain: state.return_held_after_drain,
            invalidation_pending: state.invalidation_pending(),
            supervision: state.supervision,
        }
    }

    fn settings(&self) -> PumpSettings {
        self.lock().settings.clone()
    }

    fn apply_settings(&self, patch: PumpSettingsPatch) -> Result<PumpSettings, PumpSettingsError> {
        let candidate = {
            let mut state = self.lock();
            let candidate = patch.merge_onto(&state.settings);

            candidate.validate().map_err(PumpSettingsError::Invalid)?;
            self.write(PumpRecord::Settings, &candidate)
                .map_err(PumpSettingsError::NotPersisted)?;

            state.schedule = Schedule::compile(&candidate, Some(&state.schedule));
            state.settings = candidate.clone();
            candidate
        };

        self.notify.notify_one();

        Ok(candidate)
    }

    fn set_override(
        &self,
        id: PumpId,
        duration_s: Option<u32>,
    ) -> Result<ChannelStatus, OverrideError> {
        validate_override(id, duration_s)?;
        let slot = id.override_slot().ok_or(OverrideError::NotOverridable)?;

        let now = Instant::now();
        let epoch = self.clock.epoch_seconds();
        let expiry = match (duration_s, epoch) {
            (None, _) => Expiry::Never,
            (Some(seconds), Some(epoch)) => Expiry::Timed {
                at: epoch + u64::from(seconds),
                deadline: now + Duration::from_secs(seconds.into()),
            },
            (Some(seconds), None) => {
                tracing::warn!(
                    pump = id.name(),
                    "override set without time sync will not survive a restart"
                );
                Expiry::At(now + Duration::from_secs(seconds.into()))
            }
        };

        let status = {
            let mut state = self.lock();
            let previous = state.overrides[slot].replace(expiry);

            if expiry.stored().is_some() {
                if let Err(reason) = self.write(PumpRecord::Overrides, &state.stored_overrides()) {
                    state.overrides[slot] = previous;
                    tracing::error!(
                        record = PumpRecord::Overrides.key(),
                        reason,
                        "override not stored"
                    );
                    return Err(OverrideError::NotPersisted(reason));
                }
            } else {
                self.persist(PumpRecord::Overrides, &state.stored_overrides());
            }

            state.channel(id, now, epoch)
        };

        self.notify.notify_one();

        Ok(status)
    }

    fn clear_override(&self, id: PumpId) -> Result<(), OverrideError> {
        let slot = id.override_slot().ok_or(OverrideError::NotOverridable)?;

        {
            let mut state = self.lock();
            let previous = state.overrides[slot].take();

            if previous.and_then(Expiry::stored).is_some() {
                if let Err(reason) = self.write(PumpRecord::Overrides, &state.stored_overrides()) {
                    state.overrides[slot] = previous;
                    tracing::error!(
                        record = PumpRecord::Overrides.key(),
                        reason,
                        "override not stored"
                    );
                    return Err(OverrideError::NotPersisted(reason));
                }
            }
        }

        self.notify.notify_one();

        Ok(())
    }

    fn reset_runtime(&self, id: PumpId) {
        self.reset_runtime_at(id, Instant::now());
    }

    fn reset_flood(&self) -> Result<(), FloodResetError> {
        self.acknowledge_flood()
    }

    fn start_top_off(&self, duration_s: u32) -> Result<(), StartError> {
        validate_top_off(duration_s).map_err(StartError::Invalid)?;
        self.start(OperationPlan::top_off(duration_s))
    }

    fn start_water_change(&self, plan: WaterChangePlan) -> Result<(), StartError> {
        plan.validate().map_err(StartError::Invalid)?;
        self.start(OperationPlan::water_change(plan))
    }

    fn resume(&self, kind: OperationKind) -> Result<(), StartError> {
        let epoch = self.clock.epoch_seconds();
        {
            let mut state = self.lock();
            if state.flood.latched {
                return Err(StartError::Blocked(Blocker::Leak));
            }
            let mut operation = match std::mem::replace(&mut state.activity, Activity::Idle) {
                Activity::Suspended(operation) if operation.plan.kind == kind => operation,
                other => {
                    state.activity = other;
                    return Err(StartError::Conflict);
                }
            };

            let ready = if state.supervision == Supervision::Unavailable {
                Err(Blocker::SupervisionUnavailable)
            } else {
                operation.ready_to_resume(state.levels)
            };
            if let Err(blocker) = ready {
                state.activity = Activity::Suspended(operation);
                return Err(StartError::Blocked(blocker));
            }

            let suspended = operation.clone();
            operation.resume();
            if let Err(reason) = self.record_ahead(&mut state, &operation, epoch) {
                tracing::error!(
                    ?kind,
                    reason,
                    "pump operation not resumed, its checkpoint could not be stored"
                );
                state.activity = Activity::Suspended(suspended);
                return Err(StartError::NotPersisted(reason));
            }

            tracing::info!(?kind, "pump operation resumed over the API");
            state.activity = Activity::Running(operation);
            state.fresh_operation = true;
            self.flush_pending(&mut state);
        }

        self.notify.notify_one();

        Ok(())
    }

    fn cancel(&self, kind: OperationKind) -> Result<(), CancelError> {
        let epoch = self.clock.epoch_seconds();

        let outcome = {
            let mut state = self.lock();
            let cancelled = match std::mem::replace(&mut state.activity, Activity::Idle) {
                Activity::Running(operation) if operation.plan.kind == kind => {
                    Some((operation, OperationResult::Aborted))
                }
                Activity::Suspended(operation) if operation.plan.kind == kind => {
                    Some((operation, OperationResult::InterruptedByReboot))
                }
                Activity::Resuming(checkpoint) if checkpoint.operation.plan.kind == kind => {
                    Some((checkpoint.operation, OperationResult::InterruptedByReboot))
                }
                other => {
                    state.activity = other;
                    if !state
                        .pending_finish
                        .is_some_and(|finish| finish.kind == kind)
                    {
                        return Ok(());
                    }
                    None
                }
            };

            if let Some((operation, result)) = cancelled {
                self.finish(&mut state, &operation, result, epoch, Instant::now());
            }
            self.flush_pending(&mut state);

            if state.invalidation_pending() {
                Err(CancelError::NotPersisted(
                    "the cancel was not recorded and is retried",
                ))
            } else {
                Ok(())
            }
        };

        self.notify.notify_one();
        outcome
    }
}

impl<S: PumpStore, C: Clock> WaterChangeActivity for PumpService<S, C> {
    fn water_change_active(&self) -> bool {
        match &self.lock().activity {
            Activity::Running(operation) => operation.plan.kind == OperationKind::WaterChange,
            Activity::Resuming(checkpoint) => {
                checkpoint.operation.plan.kind == OperationKind::WaterChange
            }
            Activity::Idle | Activity::Suspended(_) => false,
        }
    }
}

impl<S: PumpStore, C: Clock> PumpStatusSource for PumpService<S, C> {
    fn flood_status(&self) -> FloodStatus {
        self.lock().flood
    }

    fn pump_telemetry(&self) -> PumpsTelemetry {
        let state = self.lock();
        let on =
            |id: PumpId| state.outputs[id.index()].map_or(id.fail_safe_on(), |output| output.on);
        let operation = state.operation_status();

        PumpsTelemetry {
            return_pump: on(PumpId::Return),
            uv: on(PumpId::Uv),
            makeup: on(PumpId::Makeup),
            drain: on(PumpId::Drain),
            operation_kind: operation.kind,
            operation_state: operation.state,
            circulation_starved: state.circulation_starved,
            level_sensors_suspect: state.level_sensors_suspect,
            return_held_after_drain: state.return_held_after_drain,
            invalidation_pending: state.invalidation_pending(),
            supervision: state.supervision,
        }
    }

    fn last_top_off(&self) -> Option<OperationRecord> {
        self.lock().last.top_off
    }
}

#[cfg(test)]
mod tests;
