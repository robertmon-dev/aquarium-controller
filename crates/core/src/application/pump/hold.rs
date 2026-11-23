use std::time::Duration;

use tokio::time::Instant;

use super::service::PumpService;
use super::state::Activity;
use crate::domain::pump::{OperationKind, OperationResult, Phase};
use crate::ports::outbound::{Clock, PumpStore};

const BLIND_RETURN_HOLD: Duration = Duration::from_secs(1800);

impl<S: PumpStore, C: Clock> PumpService<S, C> {
    pub(super) fn hold_return_after_drain(&self, blind: bool, now: Instant) -> bool {
        let epoch = self.clock.epoch_seconds();
        let mut state = self.lock();

        let recovering_drain = match &state.activity {
            Activity::Resuming(checkpoint) => checkpoint.operation.phase == Phase::Draining,
            Activity::Suspended(operation) => operation.phase == Phase::Draining,
            _ => false,
        };
        let flooded = state
            .levels
            .is_some_and(|levels| levels.tank_overflow || levels.sump_high);
        let hold_for_recovery = recovering_drain && flooded;
        if !blind && !hold_for_recovery {
            if state.return_held_after_drain && (state.levels.is_none() || flooded) {
                return true;
            }
            if state.return_held_after_drain {
                tracing::info!("level readings are back, the return pump is released");
                state.return_held_after_drain = false;
                state.pending.hold = true;
            }
            return false;
        }

        if !state.return_held_after_drain {
            let draining = match &state.activity {
                Activity::Idle => false,
                Activity::Running(operation) | Activity::Suspended(operation) => {
                    operation.plan.kind == OperationKind::WaterChange
                }
                Activity::Resuming(checkpoint) => {
                    checkpoint.operation.plan.kind == OperationKind::WaterChange
                }
            };
            let unfinished = |result| {
                !matches!(
                    result,
                    OperationResult::SumpFull | OperationResult::Completed
                )
            };
            let ended_recently = state.water_change_ended.is_some_and(|(ended, result)| {
                unfinished(result) && now.saturating_duration_since(ended) < BLIND_RETURN_HOLD
            });
            let booted_recently = self.clock.uptime_seconds() < BLIND_RETURN_HOLD.as_secs();
            let recorded_recently = state.last.water_change.is_some_and(|record| {
                unfinished(record.result)
                    && match epoch {
                        Some(epoch) => record.finished_at.is_some_and(|finished| {
                            epoch.saturating_sub(finished) < BLIND_RETURN_HOLD.as_secs()
                        }),
                        None => booted_recently,
                    }
            });

            if draining || ended_recently || recorded_recently {
                tracing::error!(
                    "level readings lost right after a drain, holding the return pump off"
                );
                state.return_held_after_drain = true;
                state.pending.hold = true;
            }
        }

        state.return_held_after_drain
    }
}

#[cfg(test)]
mod tests;
