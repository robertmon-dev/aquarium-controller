use tokio::time::Instant;

use super::schedule::Schedule;
use crate::domain::pump::{
    litres, Blocker, ChannelFault, ChannelMode, ChannelStatus, ChannelsStatus, Checkpoint,
    LastOperations, LevelConditions, Operation, OperationEvent, OperationKind, OperationPlan,
    OperationResult, OperationState, OperationStatus, Phase, PumpId, PumpSettings, RuntimeCounters,
    SkippedWindow, StoredOverride, Supervision,
};
use crate::ports::inbound::StartError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Expiry {
    Never,
    At(Instant),
    Epoch(u64),
    Timed { at: u64, deadline: Instant },
}

impl Expiry {
    pub(super) fn stored(self) -> Option<StoredOverride> {
        match self {
            Self::Never => Some(StoredOverride { expires_at: None }),
            Self::Epoch(at) | Self::Timed { at, .. } => Some(StoredOverride {
                expires_at: Some(at),
            }),
            Self::At(_) => None,
        }
    }

    fn remaining_s(self, now: Instant, epoch: Option<u64>) -> Option<u32> {
        match self {
            Self::Never => None,
            Self::At(at) | Self::Timed { deadline: at, .. } => {
                Some(at.saturating_duration_since(now).as_millis().div_ceil(1000) as u32)
            }
            Self::Epoch(at) => epoch.map(|epoch| at.saturating_sub(epoch) as u32),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Output {
    pub(super) on: bool,
    pub(super) fault: Option<ChannelFault>,
    pub(super) blocked_by: Option<Blocker>,
}

#[derive(Debug, Clone)]
pub(super) enum Activity {
    Idle,
    Running(Operation),
    Resuming(Checkpoint),
    Suspended(Operation),
}

#[derive(Default)]
pub(super) struct Pending {
    pub(super) checkpoint: bool,
    pub(super) last: bool,
    pub(super) overrides: bool,
    pub(super) hold: bool,
    pub(super) events: Vec<OperationEvent>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum FinishWrite {
    MarkFinished,
    CopyHistory,
    InvalidateActive,
}

#[derive(Clone, Copy)]
pub(super) struct PendingFinish {
    pub(super) kind: OperationKind,
    pub(super) next: FinishWrite,
    pub(super) reported: bool,
}

pub(super) struct State {
    pub(super) flood: crate::domain::flood::FloodStatus,
    pub(super) flood_pending: bool,
    pub(super) settings: PumpSettings,
    pub(super) schedule: Schedule,
    pub(super) overrides: [Option<Expiry>; 2],
    pub(super) pending: Pending,
    pub(super) activity: Activity,
    pub(super) fresh_operation: bool,
    pub(super) levels: Option<LevelConditions>,
    pub(super) outputs: [Option<Output>; 4],
    pub(super) runtime_ms: [u64; 4],
    pub(super) reset_at: [Option<u64>; 4],
    pub(super) reset_instant: [Option<Instant>; 4],
    pub(super) last: LastOperations,
    pub(super) last_skipped: Option<SkippedWindow>,
    pub(super) last_fired_minute: Option<u64>,
    pub(super) resume_sync_epoch: Option<u64>,
    pub(super) circulation_starved: bool,
    pub(super) level_sensors_suspect: bool,
    pub(super) pending_finish: Option<PendingFinish>,
    pub(super) water_change_ended: Option<(Instant, OperationResult)>,
    pub(super) return_held_after_drain: bool,
    pub(super) supervision: Supervision,
    pub(super) checkpoint_failed: bool,
    pub(super) started_before_boot: [Option<u64>; 2],
}

impl State {
    pub(super) fn invalidation_pending(&self) -> bool {
        self.pending_finish
            .is_some_and(|finish| finish.next != FinishWrite::CopyHistory)
    }

    fn live_override(&mut self, id: PumpId, now: Instant, epoch: Option<u64>) -> Option<Expiry> {
        let slot = id.override_slot()?;
        let expired = match self.overrides[slot]? {
            Expiry::Never => false,
            Expiry::At(at) | Expiry::Timed { deadline: at, .. } => now >= at,
            Expiry::Epoch(at) => epoch.is_some_and(|epoch| epoch >= at),
        };

        if expired {
            self.overrides[slot] = None;
            self.pending.overrides = true;
        }

        self.overrides[slot]
    }

    pub(super) fn forced_off(&mut self, id: PumpId, now: Instant, epoch: Option<u64>) -> bool {
        match self.live_override(id, now, epoch) {
            None => false,
            Some(Expiry::Epoch(_)) if epoch.is_none() => id == PumpId::Uv,
            Some(_) => true,
        }
    }

    pub(super) fn stored_overrides(&self) -> [Option<StoredOverride>; 2] {
        self.overrides.map(|slot| slot.and_then(Expiry::stored))
    }

    pub(super) fn runtime(&self) -> RuntimeCounters {
        RuntimeCounters {
            runtime_s: self.runtime_ms.map(|ms| ms / 1000),
            reset_at: self.reset_at,
        }
    }

    pub(super) fn channel(
        &mut self,
        id: PumpId,
        now: Instant,
        epoch: Option<u64>,
    ) -> ChannelStatus {
        let over = self.live_override(id, now, epoch);
        let in_force = self.forced_off(id, now, epoch);
        let output = self.outputs[id.index()];

        ChannelStatus {
            on: output.map_or(id.fail_safe_on(), |output| output.on),
            mode: if in_force {
                ChannelMode::ManualOff
            } else {
                ChannelMode::Auto
            },
            override_expires_in_s: over.and_then(|expiry| expiry.remaining_s(now, epoch)),
            fault: match output {
                Some(output) => output.fault,
                None => Some(ChannelFault::Unavailable),
            },
            blocked_by: output.and_then(|output| output.blocked_by),
            runtime_s: self.runtime_ms[id.index()] / 1000,
            runtime_reset_at: self.reset_at[id.index()],
        }
    }

    pub(super) fn channels(&mut self, now: Instant, epoch: Option<u64>) -> ChannelsStatus {
        ChannelsStatus {
            return_pump: self.channel(PumpId::Return, now, epoch),
            uv: self.channel(PumpId::Uv, now, epoch),
            makeup: self.channel(PumpId::Makeup, now, epoch),
            drain: self.channel(PumpId::Drain, now, epoch),
        }
    }

    pub(super) fn operation_status(&self) -> OperationStatus {
        let (state, operation) = match &self.activity {
            Activity::Idle => return OperationStatus::IDLE,
            Activity::Running(operation) => (OperationState::from(operation.phase), operation),
            Activity::Resuming(checkpoint) => (OperationState::Resuming, &checkpoint.operation),
            Activity::Suspended(operation) => (OperationState::Suspended, operation),
        };

        let flows = self.settings.flow_lph;
        let drain_ran_s = operation.ran_s(Phase::Draining);
        let refill_ran_s = operation.ran_s(Phase::Refilling);

        OperationStatus {
            kind: Some(operation.plan.kind),
            state,
            phase_remaining_s: matches!(self.activity, Activity::Running(_))
                .then(|| operation.remaining_s()),
            drain_ran_s,
            settle_ran_s: operation.ran_s(Phase::Settling),
            refill_ran_s,
            drained_l: litres(drain_ran_s, flows.drain),
            refilled_l: litres(refill_ran_s, flows.makeup),
        }
    }

    pub(super) fn phase(&self) -> Option<Phase> {
        match &self.activity {
            Activity::Running(operation) => Some(operation.phase),
            _ => None,
        }
    }

    pub(super) fn admit(
        &mut self,
        plan: &OperationPlan,
        now: Instant,
        epoch: Option<u64>,
    ) -> Result<(), StartError> {
        if self.flood.latched {
            return Err(StartError::Blocked(Blocker::Leak));
        }
        match (&self.activity, plan.kind) {
            (Activity::Idle, _) => {}
            (Activity::Running(current), OperationKind::WaterChange)
                if current.plan.kind == OperationKind::TopOff => {}
            _ => return Err(StartError::Conflict),
        }

        if self.supervision == Supervision::Unavailable {
            return Err(StartError::Blocked(Blocker::SupervisionUnavailable));
        }

        let levels = self
            .levels
            .ok_or(StartError::Blocked(Blocker::LevelsUnavailable))?;

        if plan.kind == OperationKind::TopOff {
            if levels.reservoir_empty {
                return Err(StartError::Blocked(Blocker::ReservoirEmpty));
            }

            if levels.tank_overflow {
                return Err(StartError::Blocked(Blocker::TankOverflow));
            }

            if levels.sump_full {
                return Err(StartError::Blocked(Blocker::SumpFull));
            }
        }

        if plan.kind == OperationKind::WaterChange && levels.tank_overflow {
            return Err(StartError::Blocked(Blocker::TankOverflow));
        }

        if levels.sump_high {
            return Err(StartError::Blocked(Blocker::SumpHigh));
        }

        if plan.kind == OperationKind::WaterChange && !levels.reservoir_ready {
            return Err(StartError::Blocked(Blocker::ReservoirLow));
        }

        if self.forced_off(PumpId::Return, now, epoch) {
            return Err(StartError::Blocked(Blocker::ReturnOff));
        }

        if plan.kind == OperationKind::WaterChange && self.water_change_too_recent(epoch) {
            return Err(StartError::Blocked(Blocker::RecentWaterChange));
        }

        Ok(())
    }

    fn water_change_too_recent(&self, epoch: Option<u64>) -> bool {
        let gap_s = u64::from(self.settings.min_water_change_gap_h) * 3600;
        let started_at = self.last.last_drain_started_at();

        match (epoch, started_at) {
            (Some(now), Some(started)) if gap_s > 0 => now < started.saturating_add(gap_s),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests;
