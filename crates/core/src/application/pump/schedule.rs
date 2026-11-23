use crate::domain::cron::{CivilTime, CronExpr, REPEAT_HORIZON_S};
use crate::domain::pump::{PumpSettings, WaterChangePlan};

#[derive(Clone)]
struct FiredHour {
    local_hour: (u8, u8, u8),
    start_epoch_minute: i64,
    minutes: u64,
}

pub(super) struct Window<T> {
    cron: CronExpr,
    pub(super) plan: T,
    fired: Vec<FiredHour>,
}

impl<T> Window<T> {
    pub(super) fn due(&mut self, civil: CivilTime, epoch: u64) -> bool {
        if !self.cron.matches(civil) {
            return false;
        }

        let epoch_minute = (epoch / 60) as i64;
        let start = epoch_minute - i64::from(civil.minute);
        let local_hour = (civil.month, civil.day, civil.hour);
        let bit = 1u64 << civil.minute;
        let horizon = REPEAT_HORIZON_S / 60;
        self.fired
            .retain(|hour| hour.start_epoch_minute.abs_diff(epoch_minute) < horizon + 60);
        if self.fired.iter().any(|hour| {
            hour.local_hour == local_hour
                && hour.start_epoch_minute.abs_diff(start) < horizon
                && hour.minutes & bit != 0
        }) {
            return false;
        }

        if let Some(hour) = self
            .fired
            .iter_mut()
            .find(|hour| hour.local_hour == local_hour && hour.start_epoch_minute == start)
        {
            hour.minutes |= bit;
        } else {
            self.fired.push(FiredHour {
                local_hour,
                start_epoch_minute: start,
                minutes: bit,
            });
        }
        true
    }
}

pub(super) struct Schedule {
    pub(super) top_off: Vec<Window<u32>>,
    pub(super) water_change: Vec<Window<WaterChangePlan>>,
}

fn windows<W, T>(
    windows: &[W],
    previous: &[Window<T>],
    parts: impl Fn(&W) -> (&str, T),
) -> Vec<Window<T>> {
    windows
        .iter()
        .filter_map(|window| {
            let (cron, plan) = parts(window);
            CronExpr::parse(cron).ok().map(|cron| (cron, plan))
        })
        .enumerate()
        .map(|(index, (cron, plan))| Window {
            cron,
            plan,
            fired: previous
                .get(index)
                .map(|window| window.fired.clone())
                .unwrap_or_default(),
        })
        .collect()
}

impl Schedule {
    pub(super) fn compile(settings: &PumpSettings, previous: Option<&Self>) -> Self {
        Self {
            top_off: windows(
                &settings.top_off,
                previous.map_or(&[], |schedule| &schedule.top_off),
                |window| (window.cron.as_str(), window.duration_s),
            ),
            water_change: windows(
                &settings.water_change,
                previous.map_or(&[], |schedule| &schedule.water_change),
                |window| (window.cron.as_str(), window.plan()),
            ),
        }
    }
}

#[cfg(test)]
mod tests;
