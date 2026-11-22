use serde::{Deserialize, Serialize};

use crate::domain::cron::CronExpr;
use crate::domain::water::WaterLevelRole;

pub const MAX_TOP_OFF_S: u32 = 600;
pub const MAX_DRAIN_S: u32 = 900;
pub const MAX_SETTLE_S: u32 = 300;
pub const DEFAULT_SETTLE_S: u32 = 30;
pub const MAX_REFILL_S: u32 = 900;
pub const MAX_WINDOWS: usize = 4;
pub const MAX_CRON_LEN: usize = 64;
pub const MAX_FLOW_LPH: u32 = 10_000;
pub const MAX_RESUME_WINDOW_S: u32 = 3600;
pub const DEFAULT_RESUME_WINDOW_S: u32 = 600;
pub const MAX_WATER_CHANGE_GAP_H: u32 = 168;
pub const DEFAULT_WATER_CHANGE_GAP_H: u32 = 12;
pub const MAX_RESERVOIR_L: u32 = 10_000;

const fn default_settle_s() -> u32 {
    DEFAULT_SETTLE_S
}

const fn default_water_change_gap_h() -> u32 {
    DEFAULT_WATER_CHANGE_GAP_H
}

const fn default_refill_stop_sensor() -> Option<WaterLevelRole> {
    Some(WaterLevelRole::SumpReturnLevel)
}

pub(crate) fn nullable<'de, D, T>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WaterChangePlan {
    pub drain_s: u32,

    #[serde(default = "default_settle_s")]
    pub settle_s: u32,

    pub refill_s: u32,
}

impl WaterChangePlan {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(1..=MAX_DRAIN_S).contains(&self.drain_s) {
            return Err("drain_s must be between 1 and 900");
        }

        if self.settle_s > MAX_SETTLE_S {
            return Err("settle_s must not exceed 300");
        }

        if !(1..=MAX_REFILL_S).contains(&self.refill_s) {
            return Err("refill_s must be between 1 and 900");
        }

        Ok(())
    }
}

pub fn validate_top_off(duration_s: u32) -> Result<(), &'static str> {
    if (1..=MAX_TOP_OFF_S).contains(&duration_s) {
        Ok(())
    } else {
        Err("a top-off lasts 1 to 600 s")
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct TopOffWindow {
    pub cron: String,
    pub duration_s: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct WaterChangeWindow {
    pub cron: String,
    pub drain_s: u32,

    #[serde(default = "default_settle_s")]
    pub settle_s: u32,

    pub refill_s: u32,
}

impl WaterChangeWindow {
    pub const fn plan(&self) -> WaterChangePlan {
        WaterChangePlan {
            drain_s: self.drain_s,
            settle_s: self.settle_s,
            refill_s: self.refill_s,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FlowRates {
    #[serde(default)]
    pub makeup: Option<u32>,

    #[serde(default)]
    pub drain: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PumpSettings {
    pub top_off: Vec<TopOffWindow>,
    pub water_change: Vec<WaterChangeWindow>,
    pub flow_lph: FlowRates,
    pub resume_window_s: u32,

    #[serde(default = "default_refill_stop_sensor")]
    pub refill_stop_sensor: Option<WaterLevelRole>,

    #[serde(default = "default_water_change_gap_h")]
    pub min_water_change_gap_h: u32,

    #[serde(default)]
    pub reservoir_ready_l: Option<u32>,
}

impl PumpSettings {
    pub const DEFAULT: Self = Self {
        top_off: Vec::new(),
        water_change: Vec::new(),
        flow_lph: FlowRates {
            makeup: None,
            drain: None,
        },
        resume_window_s: DEFAULT_RESUME_WINDOW_S,
        refill_stop_sensor: default_refill_stop_sensor(),
        min_water_change_gap_h: DEFAULT_WATER_CHANGE_GAP_H,
        reservoir_ready_l: None,
    };

    pub fn drain_fits_reservoir(&self, drain_s: u32) -> bool {
        match (self.flow_lph.drain, self.reservoir_ready_l) {
            (Some(flow), Some(reservoir)) => {
                u64::from(drain_s) * u64::from(flow) <= u64::from(reservoir) * 3600
            }
            _ => true,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.top_off.len() > MAX_WINDOWS || self.water_change.len() > MAX_WINDOWS {
            return Err("at most 4 top-off and 4 water-change windows");
        }

        for cron in self
            .top_off
            .iter()
            .map(|window| &window.cron)
            .chain(self.water_change.iter().map(|window| &window.cron))
        {
            if cron.len() > MAX_CRON_LEN {
                return Err("cron must be at most 64 characters");
            }
        }

        for window in &self.top_off {
            CronExpr::parse(&window.cron)?;
            validate_top_off(window.duration_s)?;
        }

        for window in &self.water_change {
            CronExpr::parse(&window.cron)?;
            window.plan().validate()?;
        }

        for flow in [self.flow_lph.makeup, self.flow_lph.drain]
            .into_iter()
            .flatten()
        {
            if !(1..=MAX_FLOW_LPH).contains(&flow) {
                return Err("flow must be between 1 and 10000 L/h");
            }
        }

        if self.resume_window_s > MAX_RESUME_WINDOW_S {
            return Err("resume_window_s must not exceed 3600");
        }

        if !matches!(
            self.refill_stop_sensor,
            None | Some(
                WaterLevelRole::SumpReturnLevel
                    | WaterLevelRole::SumpMechanical
                    | WaterLevelRole::SumpBiological
            )
        ) {
            return Err(
                "refill_stop_sensor must be sump_return_level, sump_mechanical or sump_biological",
            );
        }

        if self.min_water_change_gap_h > MAX_WATER_CHANGE_GAP_H {
            return Err("min_water_change_gap_h must not exceed 168");
        }

        if self
            .reservoir_ready_l
            .is_some_and(|litres| !(1..=MAX_RESERVOIR_L).contains(&litres))
        {
            return Err("reservoir_ready_l must be between 1 and 10000");
        }

        if self
            .water_change
            .iter()
            .any(|window| !self.drain_fits_reservoir(window.drain_s))
        {
            return Err("a water-change window drains more than reservoir_ready_l holds");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct PumpSettingsPatch {
    #[serde(default)]
    pub top_off: Option<Vec<TopOffWindow>>,

    #[serde(default)]
    pub water_change: Option<Vec<WaterChangeWindow>>,

    #[serde(default)]
    pub flow_lph: Option<FlowRates>,

    #[serde(default)]
    pub resume_window_s: Option<u32>,

    #[serde(default, deserialize_with = "nullable")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub refill_stop_sensor: Option<Option<WaterLevelRole>>,

    #[serde(default)]
    pub min_water_change_gap_h: Option<u32>,

    #[serde(default, deserialize_with = "nullable")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub reservoir_ready_l: Option<Option<u32>>,
}

impl PumpSettingsPatch {
    pub fn merge_onto(self, base: &PumpSettings) -> PumpSettings {
        PumpSettings {
            top_off: self.top_off.unwrap_or_else(|| base.top_off.clone()),
            water_change: self
                .water_change
                .unwrap_or_else(|| base.water_change.clone()),
            flow_lph: self.flow_lph.unwrap_or(base.flow_lph),
            resume_window_s: self.resume_window_s.unwrap_or(base.resume_window_s),
            refill_stop_sensor: self.refill_stop_sensor.unwrap_or(base.refill_stop_sensor),
            min_water_change_gap_h: self
                .min_water_change_gap_h
                .unwrap_or(base.min_water_change_gap_h),
            reservoir_ready_l: self.reservoir_ready_l.unwrap_or(base.reservoir_ready_l),
        }
    }
}

#[cfg(test)]
mod tests;
