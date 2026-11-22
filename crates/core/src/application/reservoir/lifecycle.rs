use tokio::time::Instant;

use super::service::ReservoirService;
use super::state::State;
use super::storage::write;
use crate::domain::pump::litres;
use crate::domain::reservoir::{
    FaultRecord, Fill, FillEvent, FillFault, FillResult, FillTrigger, RunningMarker,
};
use crate::ports::outbound::{Clock, ReservoirRecord, ReservoirStore};

impl<S: ReservoirStore, C: Clock> ReservoirService<S, C> {
    pub(super) fn raise(&self, state: &mut State, fault: FillFault, at: Option<u64>) {
        state.fault = Some(FaultRecord { fault, at });
        tracing::error!(?fault, "barrel locked by a fault");
        state.fault_pending = match write(&self.store, ReservoirRecord::Fault, &state.fault) {
            Ok(()) => false,
            Err(reason) => {
                tracing::error!(reason, "barrel fault not stored, retrying");
                true
            }
        };
    }

    pub(super) fn begin(
        &self,
        state: &mut State,
        trigger: FillTrigger,
        epoch: Option<u64>,
    ) -> Result<(), &'static str> {
        let marker = RunningMarker {
            trigger,
            started_at: epoch,
        };
        if let Err(reason) = write(&self.store, ReservoirRecord::Running, &marker) {
            tracing::error!(
                reason,
                "barrel fill not started, its marker could not be stored"
            );
            return Err(reason);
        }
        state.fill = Some(Fill::start(trigger, epoch, state.settings.fill_plan()));
        state.just_started = true;
        state.mixed_at = None;
        state.invalidation_pending = false;
        state.last_skipped = None;
        state.debounce.reset();
        for slot in &mut state.overrides {
            if slot.is_some_and(|value| value.on) {
                *slot = None;
            }
        }
        tracing::info!(?trigger, "barrel fill started");
        Ok(())
    }

    pub(super) fn finish(
        &self,
        state: &mut State,
        fill: Fill,
        result: FillResult,
        epoch: Option<u64>,
        now: Instant,
    ) {
        let record = fill.record(result, epoch, state.fresh_tds(now, self.timing.tds_stale));
        state.fill = None;
        state.mixed_at = None;
        state.last = Some(record);
        state.debounce.reset();

        if let Some(fault) = result.fault() {
            self.raise(state, fault, epoch);
        }
        if fill.trigger() == FillTrigger::Auto && result.latches_auto() {
            state.auto_latched = true;
        }
        if let Err(reason) = write(&self.store, ReservoirRecord::Last, &record) {
            tracing::warn!(reason, "barrel fill result not stored");
        }
        state.invalidation_pending = true;

        let flows = state.settings.flow_lph;
        state.events.push(FillEvent {
            record,
            ro_l: litres(record.ro_s, flows.ro),
            tap_l: litres(record.tap_s, flows.tap),
        });
        tracing::info!(?result, "barrel fill finished");
    }
}
