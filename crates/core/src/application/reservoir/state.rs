use std::time::Duration;

use tokio::time::Instant;

use super::storage::Recovery;
use crate::domain::pump::Supervision;
use crate::domain::reservoir::{
    admit, inlet_blocker, DecisionInput, Demand, DutyMeter, FaultRecord, Fill, FillEvent,
    FillRecord, Inlet, OverrideTarget, RefillDebounce, ReservoirBlocker, ReservoirConditions,
    ReservoirSettings, SkipReason, SkippedFill, TDS_WAIT_MS,
};
use crate::ports::outbound::TdsSample;

#[derive(Debug, Clone, Copy)]
pub(super) struct Override {
    pub(super) on: bool,
    pub(super) until: Option<Instant>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Output {
    pub(super) on: bool,
    pub(super) error: bool,
    pub(super) blocked_by: Option<ReservoirBlocker>,
}

pub(super) struct State {
    pub(super) settings: ReservoirSettings,
    pub(super) fill: Option<Fill>,
    pub(super) cycle_id: u64,
    pub(super) just_started: bool,
    pub(super) mixed_at: Option<Instant>,
    pub(super) overrides: [Option<Override>; 3],
    pub(super) outputs: [Output; 4],
    pub(super) outputs_uncertain: bool,
    pub(super) runtime_ms: [u64; 4],
    pub(super) conditions: Option<ReservoirConditions>,
    pub(super) leak: Option<bool>,
    pub(super) water_change_active: bool,
    pub(super) supervision: Supervision,
    pub(super) held_after_reboot: bool,
    pub(super) auto_latched: bool,
    pub(super) fault: Option<FaultRecord>,
    pub(super) fault_pending: bool,
    pub(super) last_pending: bool,
    pub(super) last: Option<FillRecord>,
    pub(super) last_skipped: Option<SkippedFill>,
    pub(super) invalidation_pending: bool,
    pub(super) debounce: RefillDebounce,
    pub(super) duty: [DutyMeter; 2],
    pub(super) tds: Option<TdsSample>,
    pub(super) events: Vec<FillEvent>,
}

impl State {
    pub(super) fn new(
        settings: ReservoirSettings,
        runtime_ms: [u64; 4],
        recovery: Recovery,
    ) -> Self {
        Self {
            settings,
            fill: None,
            cycle_id: recovery.cycle_id,
            just_started: false,
            mixed_at: None,
            overrides: [None; 3],
            outputs: [Output::default(); 4],
            outputs_uncertain: false,
            runtime_ms,
            conditions: None,
            leak: None,
            water_change_active: false,
            supervision: Supervision::Off,
            held_after_reboot: recovery.held_after_reboot,
            auto_latched: recovery.auto_latched,
            fault: recovery.fault,
            fault_pending: recovery.fault_pending,
            last_pending: recovery.last_pending,
            last: recovery.last,
            last_skipped: None,
            invalidation_pending: recovery.invalidation_pending,
            debounce: RefillDebounce::default(),
            duty: [DutyMeter::default(); 2],
            tds: None,
            events: Vec::new(),
        }
    }

    fn reading(&self, now: Instant, stale: Duration) -> Option<TdsSample> {
        self.tds
            .filter(|sample| now.saturating_duration_since(sample.measured_at) <= stale)
    }

    pub(super) fn fresh_tds(&self, now: Instant, stale: Duration) -> Option<f32> {
        self.reading(now, stale).and_then(|sample| sample.ppm)
    }

    pub(super) fn mixed_tds(&self, now: Instant, stale: Duration) -> Option<f32> {
        let mixed_at = self.mixed_at?;
        self.reading(now, stale)
            .filter(|sample| sample.measured_at >= mixed_at)
            .and_then(|sample| sample.ppm)
    }

    pub(super) fn leaking(&self) -> bool {
        self.leak == Some(true)
    }

    fn override_on(&self, target: OverrideTarget) -> bool {
        self.overrides[target.index()].is_some_and(|slot| slot.on)
    }

    fn override_off(&self, target: OverrideTarget) -> bool {
        self.overrides[target.index()].is_some_and(|slot| !slot.on)
    }

    fn demand(&self) -> Demand {
        let wanted = match self.fill {
            Some(fill) => fill.demand(),
            None => Demand {
                inlet: Inlet::ALL
                    .into_iter()
                    .find(|inlet| self.override_on(inlet.target())),
                mixer: self.override_on(OverrideTarget::Mixer),
            },
        };
        Demand {
            inlet: wanted
                .inlet
                .filter(|inlet| !self.override_off(inlet.target())),
            mixer: wanted.mixer && !self.override_off(OverrideTarget::Mixer),
        }
    }

    pub(super) fn decision_input(&self, ro_valve_open_ms: u64) -> DecisionInput {
        DecisionInput {
            conditions: self.conditions,
            supervision: self.supervision,
            fault: self.fault.is_some(),
            outputs_uncertain: self.outputs_uncertain,
            water_change_active: self.water_change_active,
            resting: self.duty.map(|meter| meter.resting()),
            demand: self.demand(),
            ro_valve_open_ms,
        }
    }

    pub(super) fn admit_fill(&self, now: Instant) -> Result<(), ReservoirBlocker> {
        let input = DecisionInput {
            resting: [false; 2],
            ..self.decision_input(0)
        };
        let conditions = admit(&input)?;
        if let Some(blocker) = inlet_blocker(&input, conditions, Inlet::Ro) {
            return Err(blocker);
        }
        let tds_missing = self.settings.target_tds_ppm.is_some()
            && self
                .fresh_tds(now, Duration::from_millis(TDS_WAIT_MS))
                .is_none();
        if tds_missing {
            Err(ReservoirBlocker::TdsUnavailable)
        } else {
            Ok(())
        }
    }

    pub(super) fn admit_override(&self, target: OverrideTarget) -> Result<(), ReservoirBlocker> {
        let input = self.decision_input(0);
        let conditions = admit(&input)?;
        let refusal = match target.inlet() {
            None => conditions.empty.then_some(ReservoirBlocker::ReservoirEmpty),
            Some(inlet) if self.override_on(inlet.other().target()) => {
                Some(ReservoirBlocker::Interlock)
            }
            Some(inlet) => inlet_blocker(&input, conditions, inlet),
        };
        refusal.map_or(Ok(()), Err)
    }

    pub(super) fn auto_skip(&self) -> Option<SkipReason> {
        if self.held_after_reboot {
            return Some(SkipReason::HeldAfterReboot);
        }
        if self.auto_latched {
            return Some(SkipReason::AutoLatched);
        }
        match admit(&self.decision_input(0)) {
            Err(ReservoirBlocker::Fault) => Some(SkipReason::Fault),
            Err(ReservoirBlocker::Leak) => Some(SkipReason::Leak),
            Err(ReservoirBlocker::SupervisionUnavailable) => {
                Some(SkipReason::SupervisionUnavailable)
            }
            _ if self.water_change_active => Some(SkipReason::WaterChange),
            _ => None,
        }
    }

    pub(super) fn skip(&mut self, reason: SkipReason, at: Option<u64>) -> bool {
        if self
            .last_skipped
            .is_some_and(|skipped| skipped.reason == reason)
        {
            return false;
        }
        tracing::warn!(?reason, "automatic barrel fill skipped");
        self.last_skipped = Some(SkippedFill { reason, at });
        true
    }
}
