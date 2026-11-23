use std::time::Duration;

use tokio::time::Instant;

use super::service::ReservoirService;
use super::state::State;
use crate::domain::reservoir::{
    admit, DecisionInput, Fill, FillFault, FillInput, FillPhase, FillResult, FillStep, FillTrigger,
    Inlet, ReservoirChannel, ReservoirConditions, SkipReason,
};
use crate::ports::outbound::{Clock, ReservoirStore, TdsSample};

#[derive(Debug, Clone, Copy)]
pub struct Observation {
    pub conditions: Option<ReservoirConditions>,
    pub leak: Option<bool>,
    pub water_change_active: bool,
    pub tds: Option<TdsSample>,
    pub ran: [bool; 4],
    pub uncertain: bool,
    pub on_failed: bool,
    pub ro_valve_open_ms: u64,
    pub elapsed: Duration,
    pub now: Instant,
}

impl Observation {
    pub fn idle(conditions: Option<ReservoirConditions>, now: Instant) -> Self {
        Self {
            conditions,
            leak: conditions.and_then(|conditions| conditions.leak),
            water_change_active: false,
            tds: None,
            ran: [false; 4],
            uncertain: false,
            on_failed: false,
            ro_valve_open_ms: 0,
            elapsed: Duration::ZERO,
            now,
        }
    }
}

pub struct Plan {
    pub input: DecisionInput,
    pub changed: bool,
}

impl<S: ReservoirStore, C: Clock> ReservoirService<S, C> {
    pub fn observe(&self, observation: Observation) -> Plan {
        let epoch = self.clock.epoch_seconds();
        let now = observation.now;
        let elapsed_ms = observation.elapsed.as_millis() as u64;
        let mut state = self.lock();
        let mut changed = false;

        state.conditions = observation.conditions;
        state.leak = observation.leak;
        state.outputs_uncertain = observation.uncertain;
        changed |= std::mem::replace(
            &mut state.water_change_active,
            observation.water_change_active,
        ) != observation.water_change_active;
        if observation.tds.is_some() {
            state.tds = observation.tds;
        }

        for channel in ReservoirChannel::ALL {
            if observation.ran[channel.index()] {
                let runtime = &mut state.runtime_ms[channel.index()];
                *runtime = runtime.saturating_add(elapsed_ms);
            }
        }
        let limits = state.settings.duty();
        for inlet in Inlet::ALL {
            let meter = &mut state.duty[inlet.index()];
            let was_resting = meter.resting();
            meter.advance(observation.ran[inlet.valve().index()], elapsed_ms, limits);
            changed |= was_resting != meter.resting();
        }
        state.debounce.advance(observation.conditions, elapsed_ms);

        for slot in &mut state.overrides {
            if slot.is_some_and(|value| value.until.is_some_and(|until| now >= until)) {
                *slot = None;
                changed = true;
            }
        }

        if state.auto_latched
            && state
                .conditions
                .is_some_and(|conditions| conditions.full && !conditions.full_faulty)
        {
            state.auto_latched = false;
            state.last_pending = state.last.is_some();
            changed = true;
        }

        changed |= match state.fill {
            Some(fill) => self.advance(&mut state, fill, &observation, epoch),
            None => self.idle(&mut state, epoch),
        };

        let input = state.decision_input(observation.ro_valve_open_ms);
        self.flush_pending(&mut state);

        Plan { input, changed }
    }

    fn advance(
        &self,
        state: &mut State,
        mut fill: Fill,
        observation: &Observation,
        epoch: Option<u64>,
    ) -> bool {
        let just_started = std::mem::take(&mut state.just_started);
        let elapsed_ms = if just_started {
            0
        } else {
            observation.elapsed.as_millis() as u64
        };
        let healthy = !observation.uncertain && (just_started || !observation.on_failed);
        let step = if state.leaking() {
            FillStep::Finished(FillResult::Leak)
        } else {
            match admit(&state.decision_input(observation.ro_valve_open_ms)) {
                Ok(conditions) if healthy => fill.advance(FillInput {
                    conditions,
                    elapsed_ms,
                    delivered: Inlet::ALL.map(|inlet| observation.ran[inlet.valve().index()]),
                    mixed: observation.ran[ReservoirChannel::Mixer.index()],
                    tds: state.mixed_tds(observation.now, self.timing.tds_stale),
                }),
                _ => FillStep::Finished(FillResult::Blocked),
            }
        };

        match step {
            FillStep::Continue => {
                state.fill = Some(fill);
                false
            }
            FillStep::PhaseChanged => {
                if fill.phase() == FillPhase::Measuring {
                    state.mixed_at = Some(observation.now);
                }
                tracing::info!(phase = ?fill.phase(), "barrel fill phase");
                state.fill = Some(fill);
                true
            }
            FillStep::Finished(result) => {
                self.finish(state, fill, result, epoch, observation.now);
                true
            }
        }
    }

    fn idle(&self, state: &mut State, epoch: Option<u64>) -> bool {
        if state.leaking() && state.fault.is_none() {
            self.raise(state, FillFault::Leak, epoch);
            return true;
        }
        if !state.settings.auto_fill || !state.debounce.due(state.settings.refill_debounce_s) {
            return false;
        }
        match state.auto_skip() {
            Some(reason) => state.skip(reason, epoch),
            None => match self.begin(state, FillTrigger::Auto, epoch) {
                Ok(()) => true,
                Err(_) => {
                    state.debounce.reset();
                    state.skip(SkipReason::Storage, epoch)
                }
            },
        }
    }
}

#[cfg(test)]
mod safety_tests;
#[cfg(test)]
mod tests;
