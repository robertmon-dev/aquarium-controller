use core::str::FromStr;

use serde::{Deserialize, Serialize};

pub const MIN_TARGET_CELSIUS: f32 = 15.0;
pub const MAX_TARGET_CELSIUS: f32 = 32.0;
pub const MIN_HYSTERESIS_CELSIUS: f32 = 0.1;
pub const MAX_HYSTERESIS_CELSIUS: f32 = 2.0;
pub const MAX_CUTOFF_CELSIUS: f32 = 35.0;
pub const MAX_ON_OVERRIDE_S: u32 = 3600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum HeaterId {
    Tank,
    Sump,
}

impl HeaterId {
    pub const ALL: [Self; 2] = [Self::Tank, Self::Sump];

    pub const fn index(self) -> usize {
        match self {
            Self::Tank => 0,
            Self::Sump => 1,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Tank => "tank",
            Self::Sump => "sump",
        }
    }
}

impl FromStr for HeaterId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|id| id.name() == value)
            .ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ZoneSettings {
    pub target_celsius: f32,
    pub hysteresis_celsius: f32,
}

impl ZoneSettings {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !(MIN_TARGET_CELSIUS..=MAX_TARGET_CELSIUS).contains(&self.target_celsius) {
            return Err("target temperature must be between 15 and 32 degrees Celsius");
        }

        if !(MIN_HYSTERESIS_CELSIUS..=MAX_HYSTERESIS_CELSIUS).contains(&self.hysteresis_celsius) {
            return Err("hysteresis must be between 0.1 and 2.0 degrees Celsius");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct HeaterSettings {
    pub tank: ZoneSettings,
    pub sump: ZoneSettings,
    pub cutoff_celsius: f32,
}

impl HeaterSettings {
    pub const fn zone(&self, id: HeaterId) -> ZoneSettings {
        match id {
            HeaterId::Tank => self.tank,
            HeaterId::Sump => self.sump,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        self.tank.validate()?;
        self.sump.validate()?;

        if !self.cutoff_celsius.is_finite() || self.cutoff_celsius > MAX_CUTOFF_CELSIUS {
            return Err("cutoff must not exceed 35 degrees Celsius");
        }

        if self.cutoff_celsius <= self.tank.target_celsius
            || self.cutoff_celsius <= self.sump.target_celsius
        {
            return Err("cutoff must sit above both target temperatures");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct HeaterSettingsPatch {
    #[serde(default)]
    pub tank: Option<ZoneSettings>,

    #[serde(default)]
    pub sump: Option<ZoneSettings>,

    #[serde(default)]
    pub cutoff_celsius: Option<f32>,
}

impl HeaterSettingsPatch {
    pub fn merge_onto(&self, base: HeaterSettings) -> HeaterSettings {
        HeaterSettings {
            tank: self.tank.unwrap_or(base.tank),
            sump: self.sump.unwrap_or(base.sump),
            cutoff_celsius: self.cutoff_celsius.unwrap_or(base.cutoff_celsius),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum OverrideState {
    On,
    Off,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum HeaterMode {
    Auto,
    ManualOn,
    ManualOff,
}

impl HeaterMode {
    pub const fn from_override(over: Option<OverrideState>) -> Self {
        match over {
            None => Self::Auto,
            Some(OverrideState::On) => Self::ManualOn,
            Some(OverrideState::Off) => Self::ManualOff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum HeaterFault {
    SensorFault,
    OverTemperature,
    Unavailable,
    OutputError,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct HeaterStatus {
    pub active: bool,
    pub mode: HeaterMode,
    pub fault: Option<HeaterFault>,
    pub override_expires_in_s: Option<u32>,
    pub target_celsius: f32,
}

impl HeaterStatus {
    pub const fn unavailable(target_celsius: f32) -> Self {
        Self {
            active: false,
            mode: HeaterMode::Auto,
            fault: Some(HeaterFault::Unavailable),
            override_expires_in_s: None,
            target_celsius,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct HeatersStatus {
    pub tank: HeaterStatus,
    pub sump: HeaterStatus,
}

impl HeatersStatus {
    pub const UNAVAILABLE: Self = Self {
        tank: HeaterStatus::unavailable(0.0),
        sump: HeaterStatus::unavailable(0.0),
    };

    pub const fn get(&self, id: HeaterId) -> HeaterStatus {
        match id {
            HeaterId::Tank => self.tank,
            HeaterId::Sump => self.sump,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub on: bool,
    pub fault: Option<HeaterFault>,
}

pub fn decide(
    reading: Option<f32>,
    zone: ZoneSettings,
    cutoff_celsius: f32,
    forced: Option<OverrideState>,
    previously_on: bool,
) -> Decision {
    let Some(celsius) = reading.filter(|value| value.is_finite()) else {
        return Decision {
            on: false,
            fault: Some(HeaterFault::SensorFault),
        };
    };

    if celsius >= cutoff_celsius || cutoff_celsius.is_nan() {
        return Decision {
            on: false,
            fault: Some(HeaterFault::OverTemperature),
        };
    }

    let on = match forced {
        Some(OverrideState::On) => true,
        Some(OverrideState::Off) => false,
        None if celsius < zone.target_celsius - zone.hysteresis_celsius => true,
        None if celsius >= zone.target_celsius => false,
        None => previously_on,
    };

    Decision { on, fault: None }
}

pub fn validate_override(
    state: OverrideState,
    duration_s: Option<u32>,
) -> Result<(), &'static str> {
    match (state, duration_s) {
        (_, Some(0)) => Err("duration must be positive"),
        (OverrideState::On, None) => Err("an ON override needs duration_s"),
        (OverrideState::On, Some(seconds)) if seconds > MAX_ON_OVERRIDE_S => {
            Err("an ON override lasts at most 3600 s")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
