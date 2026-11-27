use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, MissedTickBehavior};

use super::sleep_until_some;
use crate::domain::lighting::{resolve, ChannelLevels, LightingPalette, LightingSchedule};
use crate::domain::metrics::{LightingMetricsDto, LightingProfile};
use crate::ports::outbound::{Clock, LightingOutputs, LightingOverrideSource};

pub async fn lighting_loop<C, S, O>(
    clock: C,
    overrides: S,
    mut outputs: O,
    api_changed: Arc<Notify>,
    republish: Arc<Notify>,
    interval: Duration,
) where
    C: Clock,
    S: LightingOverrideSource,
    O: LightingOutputs,
{
    let schedule = LightingSchedule::new();
    let mut last_levels: Option<ChannelLevels> = None;
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    let mut announce = false;

    loop {
        let scheduled = match (clock.local_minute_of_day(), clock.local_month()) {
            (Some(minute), Some(month)) => {
                schedule.evaluate(minute, month, &LightingPalette::DEFAULT)
            }
            _ => LightingMetricsDto::scheduled(LightingProfile::Off, ChannelLevels::OFF),
        };

        let active_override = overrides.active_override();
        let resolved = resolve(
            scheduled,
            active_override.as_ref(),
            &LightingPalette::DEFAULT,
        );
        let levels = resolved.channels;

        let mut changed = false;
        if last_levels != Some(levels) {
            match outputs.set_levels(levels) {
                Ok(()) => {
                    last_levels = Some(levels);
                    changed = true;
                }
                Err(error) => tracing::warn!(%error, "failed to update lighting outputs"),
            }
        }

        if changed || announce {
            republish.notify_one();
        }

        let wake_at = active_override
            .and_then(|over| over.expires_in_s)
            .map(|secs| Instant::now() + Duration::from_secs(u64::from(secs) + 1));

        announce = tokio::select! {
            biased;
            () = api_changed.notified() => true,
            () = sleep_until_some(wake_at) => true,
            _ = ticker.tick() => false,
        };
    }
}
