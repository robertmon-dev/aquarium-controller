use std::time::Duration;

use tokio::time::Instant;

use super::service::ReservoirService;
use super::state::Override;
use super::storage::write;
use crate::domain::reservoir::{
    validate_override, ChannelMode, FaultRecord, FillPhase, FillResult, FillStatus, FillTrigger,
    Inlet, InletRest, OverrideTarget, ReservoirBlocker, ReservoirChannel, ReservoirChannelStatus,
    ReservoirChannelTelemetry, ReservoirChannels, ReservoirSettings, ReservoirSettingsPatch,
    ReservoirStatus, ReservoirTelemetry,
};
use crate::ports::inbound::{
    FaultClearError, FillCancelError, FillStartError, ReservoirControl, ReservoirOverrideError,
    ReservoirSettingsError,
};
use crate::ports::outbound::{Clock, ReservoirRecord, ReservoirStatusSource, ReservoirStore};

impl<S: ReservoirStore, C: Clock> ReservoirService<S, C> {
    pub(super) fn status_at(&self, now: Instant) -> ReservoirStatus {
        let state = self.lock();
        let channel = |channel: ReservoirChannel| {
            let output = state.outputs[channel.index()];
            let slot = state.overrides[channel.target().index()];
            ReservoirChannelStatus {
                on: output.on,
                mode: match slot {
                    Some(value) if value.on => ChannelMode::ManualOn,
                    Some(_) => ChannelMode::ManualOff,
                    None => ChannelMode::Auto,
                },
                override_expires_in_s: slot.and_then(|value| value.until).map(|until| {
                    until
                        .saturating_duration_since(now)
                        .as_secs()
                        .min(u64::from(u32::MAX)) as u32
                }),
                blocked_by: output.blocked_by,
                output_error: output.error,
                runtime_s: state.runtime_ms[channel.index()] / 1000,
            }
        };

        ReservoirStatus {
            channels: ReservoirChannels {
                ro_valve: channel(ReservoirChannel::RoValve),
                booster: channel(ReservoirChannel::Booster),
                tap_valve: channel(ReservoirChannel::TapValve),
                mixer: channel(ReservoirChannel::Mixer),
            },
            fill: state.fill.as_ref().map(FillStatus::from),
            conditions: state.conditions,
            water_change_active: state.water_change_active,
            target_tds_ppm: state.settings.target_tds_ppm,
            fault: state.fault,
            held_after_reboot: state.held_after_reboot,
            auto_latched: state.auto_latched,
            invalidation_pending: state.invalidation_pending,
            supervision: state.supervision,
            rest_remaining_s: InletRest {
                ro: state.duty[Inlet::Ro.index()].rest_remaining_s(),
                tap: state.duty[Inlet::Tap.index()].rest_remaining_s(),
            },
            last: state.last,
            last_skipped: state.last_skipped,
        }
    }

    pub(super) fn set_override_at(
        &self,
        target: OverrideTarget,
        on: bool,
        duration_s: Option<u32>,
        now: Instant,
    ) -> Result<ReservoirStatus, ReservoirOverrideError> {
        validate_override(on, duration_s).map_err(ReservoirOverrideError::Invalid)?;
        {
            let mut state = self.lock();
            if on {
                if state.fill.is_some() {
                    return Err(ReservoirOverrideError::Busy);
                }
                state
                    .admit_override(target)
                    .map_err(ReservoirOverrideError::Blocked)?;
            }
            state.overrides[target.index()] = Some(Override {
                on,
                until: duration_s.map(|seconds| now + Duration::from_secs(u64::from(seconds))),
            });
        }
        self.notify.notify_one();
        Ok(self.status_at(now))
    }

    pub(super) fn start_fill_at(&self, now: Instant) -> Result<(), FillStartError> {
        let epoch = self.clock.epoch_seconds();
        {
            let mut state = self.lock();
            if state.fill.is_some() {
                return Err(FillStartError::Conflict);
            }
            state.admit_fill(now).map_err(FillStartError::Blocked)?;
            self.begin(&mut state, FillTrigger::Manual, epoch)
                .map_err(FillStartError::NotPersisted)?;
            state.held_after_reboot = false;
            state.auto_latched = false;
        }
        self.notify.notify_one();
        Ok(())
    }

    pub(super) fn cancel_fill_at(&self, now: Instant) -> Result<(), FillCancelError> {
        let epoch = self.clock.epoch_seconds();
        let pending = {
            let mut state = self.lock();
            if let Some(fill) = state.fill {
                self.finish(&mut state, fill, FillResult::Aborted, epoch, now);
            }
            self.flush_pending(&mut state);
            state.invalidation_pending
        };
        self.notify.notify_one();
        if pending {
            Err(FillCancelError::NotPersisted(
                "the barrel fill marker is still stored",
            ))
        } else {
            Ok(())
        }
    }
}

impl<S: ReservoirStore, C: Clock> ReservoirControl for ReservoirService<S, C> {
    fn status(&self) -> ReservoirStatus {
        self.status_at(Instant::now())
    }

    fn settings(&self) -> ReservoirSettings {
        self.lock().settings
    }

    fn apply_settings(
        &self,
        patch: ReservoirSettingsPatch,
    ) -> Result<ReservoirSettings, ReservoirSettingsError> {
        let next = {
            let mut state = self.lock();
            let next = patch.merge_onto(&state.settings);
            next.validate().map_err(ReservoirSettingsError::Invalid)?;
            write(&self.store, ReservoirRecord::Settings, &next)
                .map_err(ReservoirSettingsError::NotPersisted)?;
            state.settings = next;
            next
        };
        self.notify.notify_one();
        Ok(next)
    }

    fn set_override(
        &self,
        target: OverrideTarget,
        on: bool,
        duration_s: Option<u32>,
    ) -> Result<ReservoirStatus, ReservoirOverrideError> {
        self.set_override_at(target, on, duration_s, Instant::now())
    }

    fn clear_override(&self, target: OverrideTarget) {
        self.lock().overrides[target.index()] = None;
        self.notify.notify_one();
    }

    fn reset_runtime(&self, channel: ReservoirChannel) {
        {
            let mut state = self.lock();
            state.runtime_ms[channel.index()] = 0;
        }
        self.flush_runtime();
        self.notify.notify_one();
    }

    fn start_fill(&self) -> Result<(), FillStartError> {
        self.start_fill_at(Instant::now())
    }

    fn cancel_fill(&self) -> Result<(), FillCancelError> {
        self.cancel_fill_at(Instant::now())
    }

    fn clear_fault(&self) -> Result<(), FaultClearError> {
        {
            let mut state = self.lock();
            if state.fault.is_none() {
                return Ok(());
            }
            if state.leaking() {
                return Err(FaultClearError::Blocked(ReservoirBlocker::Leak));
            }
            write(&self.store, ReservoirRecord::Fault, &None::<FaultRecord>)
                .map_err(FaultClearError::NotPersisted)?;
            state.fault = None;
            state.fault_pending = false;
        }
        tracing::info!("barrel fault cleared");
        self.notify.notify_one();
        Ok(())
    }
}

impl<S: ReservoirStore, C: Clock> ReservoirStatusSource for ReservoirService<S, C> {
    fn reservoir_telemetry(&self) -> ReservoirTelemetry {
        let state = self.lock();
        let channel = |id: ReservoirChannel| {
            let output = state.outputs[id.index()];
            ReservoirChannelTelemetry {
                on: output.on,
                blocked_by: output.blocked_by,
                output_error: output.error,
            }
        };
        let channels = ReservoirChannels {
            ro_valve: channel(ReservoirChannel::RoValve),
            booster: channel(ReservoirChannel::Booster),
            tap_valve: channel(ReservoirChannel::TapValve),
            mixer: channel(ReservoirChannel::Mixer),
        };
        ReservoirTelemetry {
            channels: Some(channels),
            dosing_water: state.fill.and_then(|fill| fill.dosing()),
            ro_valve: channels.ro_valve.on,
            booster: channels.booster.on,
            tap_valve: channels.tap_valve.on,
            mixer: channels.mixer.on,
            phase: state.fill.map_or(FillPhase::Idle, |fill| fill.phase()),
            tds_ppm: state.fill.and_then(|fill| fill.last_tds()),
            target_tds_ppm: state.settings.target_tds_ppm,
            elapsed_s: state.fill.map_or(0, |fill| fill.elapsed_s()),
            water_change_active: state.water_change_active,
            held_after_reboot: state.held_after_reboot,
            auto_latched: state.auto_latched,
            fault: state.fault.map(|record| record.fault),
            supervision: state.supervision,
            last_result: state.last.map(|record| record.result),
        }
    }
}

#[cfg(test)]
mod tests;
