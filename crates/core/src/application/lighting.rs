use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Notify;

use crate::domain::lighting::{resolve, ChannelLevels, LightingSchedule};
use crate::domain::metrics::{LightingMetricsDto, LightingProfile};
use crate::ports::outbound::{Clock, LightingOutputs, LightingOverrideSource};

pub async fn lighting_loop<C, S, O>(
    clock: C,
    overrides: S,
    mut outputs: O,
    change_signal: Arc<Notify>,
    interval: Duration,
) where
    C: Clock,
    S: LightingOverrideSource,
    O: LightingOutputs,
{
    let schedule = LightingSchedule::new();
    let mut last_levels: Option<ChannelLevels> = None;

    loop {
        let scheduled = match (clock.local_minute_of_day(), clock.local_month()) {
            (Some(minute), Some(month)) => schedule.evaluate(minute, month),
            _ => LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            },
        };

        let resolved = resolve(scheduled, overrides.active_override().as_ref());
        let levels = ChannelLevels::from_metrics(&resolved);

        if last_levels != Some(levels) {
            match outputs.set_levels(levels) {
                Ok(()) => last_levels = Some(levels),
                Err(error) => tracing::warn!(%error, "failed to update lighting outputs"),
            }
        }

        tokio::select! {
            _ = change_signal.notified() => {}
            _ = tokio::time::sleep(interval) => {}
        }
    }
}
