use serde::{Deserialize, Serialize};

use super::channels::Inlet;
use super::duty::DutyLimits;
use crate::domain::pump::nullable;

pub const MAX_OVERRIDE_S: u32 = 600;
pub const MAX_TDS_PPM: u16 = 2000;
pub const MAX_FLOW_LPH: u32 = 10_000;
pub const OVERTIME_PCT: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct InletFlows {
    #[serde(default)]
    pub ro: Option<u32>,

    #[serde(default)]
    pub tap: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirSettings {
    pub auto_fill: bool,
    pub refill_debounce_s: u32,
    pub empty_fill_s: u32,
    pub target_tds_ppm: Option<u16>,
    pub tolerance_ppm: u16,
    pub tap_pulse_s: u16,
    pub ro_pulse_s: u16,
    pub mix_s: u16,
    pub max_duration_s: u32,
    pub inlet_max_on_s: u32,
    pub inlet_rest_s: u32,
    pub flow_lph: InletFlows,
}

impl Default for ReservoirSettings {
    fn default() -> Self {
        Self::DEFAULT
    }
}

fn within<T: PartialOrd>(
    value: T,
    low: T,
    high: T,
    reason: &'static str,
) -> Result<(), &'static str> {
    if value < low || value > high {
        Err(reason)
    } else {
        Ok(())
    }
}

impl ReservoirSettings {
    pub const DEFAULT: Self = Self {
        auto_fill: false,
        refill_debounce_s: 600,
        empty_fill_s: 7200,
        target_tds_ppm: None,
        tolerance_ppm: 10,
        tap_pulse_s: 5,
        ro_pulse_s: 120,
        mix_s: 60,
        max_duration_s: 14_400,
        inlet_max_on_s: 5400,
        inlet_rest_s: 900,
        flow_lph: InletFlows {
            ro: None,
            tap: None,
        },
    };

    pub fn validate(&self) -> Result<(), &'static str> {
        within(
            self.refill_debounce_s,
            60,
            7200,
            "refill_debounce_s must be between 60 and 7200",
        )?;
        within(
            self.empty_fill_s,
            600,
            43_200,
            "empty_fill_s must be between 600 and 43200",
        )?;
        if let Some(target) = self.target_tds_ppm {
            within(
                target,
                1,
                MAX_TDS_PPM,
                "target_tds_ppm must be between 1 and 2000",
            )?;
        }
        within(
            self.tolerance_ppm,
            1,
            200,
            "tolerance_ppm must be between 1 and 200",
        )?;
        within(
            self.tap_pulse_s,
            1,
            60,
            "tap_pulse_s must be between 1 and 60",
        )?;
        within(
            self.ro_pulse_s,
            10,
            1800,
            "ro_pulse_s must be between 10 and 1800",
        )?;
        within(self.mix_s, 10, 600, "mix_s must be between 10 and 600")?;
        within(
            self.max_duration_s,
            600,
            43_200,
            "max_duration_s must be between 600 and 43200",
        )?;
        within(
            self.inlet_max_on_s,
            600,
            14_400,
            "inlet_max_on_s must be between 600 and 14400",
        )?;
        within(
            self.inlet_rest_s,
            60,
            3600,
            "inlet_rest_s must be between 60 and 3600",
        )?;
        for flow in [self.flow_lph.ro, self.flow_lph.tap].into_iter().flatten() {
            within(
                flow,
                1,
                MAX_FLOW_LPH,
                "flow must be between 1 and 10000 L/h",
            )?;
        }
        if self.max_duration_s < self.empty_fill_s {
            return Err("max_duration_s must not be shorter than empty_fill_s");
        }
        Ok(())
    }

    pub const fn overtime_ms(&self) -> u64 {
        self.empty_fill_s as u64 * 1000 * OVERTIME_PCT / 100
    }

    pub const fn pulse_ms(&self, inlet: Inlet) -> u64 {
        let seconds = match inlet {
            Inlet::Ro => self.ro_pulse_s,
            Inlet::Tap => self.tap_pulse_s,
        };
        seconds as u64 * 1000
    }
}

impl ReservoirSettings {
    pub const fn duty(&self) -> DutyLimits {
        DutyLimits {
            max_on_ms: self.inlet_max_on_s as u64 * 1000,
            rest_ms: self.inlet_rest_s as u64 * 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ReservoirSettingsPatch {
    #[serde(default)]
    pub auto_fill: Option<bool>,
    #[serde(default)]
    pub refill_debounce_s: Option<u32>,
    #[serde(default)]
    pub empty_fill_s: Option<u32>,
    #[serde(default, deserialize_with = "nullable")]
    #[cfg_attr(feature = "ts-export", ts(optional))]
    pub target_tds_ppm: Option<Option<u16>>,
    #[serde(default)]
    pub tolerance_ppm: Option<u16>,
    #[serde(default)]
    pub tap_pulse_s: Option<u16>,
    #[serde(default)]
    pub ro_pulse_s: Option<u16>,
    #[serde(default)]
    pub mix_s: Option<u16>,
    #[serde(default)]
    pub max_duration_s: Option<u32>,
    #[serde(default)]
    pub inlet_max_on_s: Option<u32>,
    #[serde(default)]
    pub inlet_rest_s: Option<u32>,
    #[serde(default)]
    pub flow_lph: Option<InletFlows>,
}

impl ReservoirSettingsPatch {
    pub fn merge_onto(self, base: &ReservoirSettings) -> ReservoirSettings {
        ReservoirSettings {
            auto_fill: self.auto_fill.unwrap_or(base.auto_fill),
            refill_debounce_s: self.refill_debounce_s.unwrap_or(base.refill_debounce_s),
            empty_fill_s: self.empty_fill_s.unwrap_or(base.empty_fill_s),
            target_tds_ppm: self.target_tds_ppm.unwrap_or(base.target_tds_ppm),
            tolerance_ppm: self.tolerance_ppm.unwrap_or(base.tolerance_ppm),
            tap_pulse_s: self.tap_pulse_s.unwrap_or(base.tap_pulse_s),
            ro_pulse_s: self.ro_pulse_s.unwrap_or(base.ro_pulse_s),
            mix_s: self.mix_s.unwrap_or(base.mix_s),
            max_duration_s: self.max_duration_s.unwrap_or(base.max_duration_s),
            inlet_max_on_s: self.inlet_max_on_s.unwrap_or(base.inlet_max_on_s),
            inlet_rest_s: self.inlet_rest_s.unwrap_or(base.inlet_rest_s),
            flow_lph: self.flow_lph.unwrap_or(base.flow_lph),
        }
    }
}

pub fn validate_override(on: bool, duration_s: Option<u32>) -> Result<(), &'static str> {
    match (on, duration_s) {
        (_, Some(seconds)) if !(1..=MAX_OVERRIDE_S).contains(&seconds) => {
            Err("an override lasts 1 to 600 s")
        }
        (true, None) => Err("an ON override needs duration_s"),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
