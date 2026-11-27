use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::{Instant, MissedTickBehavior};

use super::super::sleep_until_some;
use super::LightingService;
use crate::domain::lighting::{resolve, ChannelLevels, LightingSchedule};
use crate::domain::metrics::{LightingMetricsDto, LightingProfile};
use crate::ports::inbound::LightingControl;
use crate::ports::outbound::{Clock, LightingOutputs, LightingPersistence};

pub async fn lighting_loop<C, O, P>(
    clock: C,
    service: Arc<LightingService<P>>,
    mut outputs: O,
    republish: Arc<Notify>,
    interval: Duration,
) where
    C: Clock,
    O: LightingOutputs,
    P: LightingPersistence,
{
    let schedule = LightingSchedule::new();
    let changes = service.change_signal();
    let mut applied: Option<ChannelLevels> = None;
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        let palette = service.palette();
        let scheduled = match (clock.local_minute_of_day(), clock.local_month()) {
            (Some(minute), Some(month)) => schedule.evaluate(minute, month, &palette),
            _ => LightingMetricsDto::scheduled(LightingProfile::Off, ChannelLevels::OFF),
        };
        let active = service.current_override();
        let resolved = resolve(scheduled, active.as_ref(), &palette);

        if applied != Some(resolved.channels) {
            match outputs.set_levels(resolved.channels) {
                Ok(()) => applied = Some(resolved.channels),
                Err(error) => tracing::warn!(%error, "failed to update lighting outputs"),
            }
        }

        if applied == Some(resolved.channels) && service.publish(resolved) {
            republish.notify_one();
        }

        let wake_at = active
            .and_then(|over| over.expires_in_s)
            .map(|secs| Instant::now() + Duration::from_secs(u64::from(secs)));

        tokio::select! {
            biased;
            () = changes.notified() => {}
            () = sleep_until_some(wake_at) => {}
            _ = ticker.tick() => {}
        }
    }
}
