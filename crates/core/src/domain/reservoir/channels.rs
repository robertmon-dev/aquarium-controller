use core::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum ReservoirChannel {
    RoValve,
    Booster,
    TapValve,
    Mixer,
}

impl ReservoirChannel {
    pub const ALL: [Self; 4] = [Self::RoValve, Self::Booster, Self::TapValve, Self::Mixer];

    pub const fn index(self) -> usize {
        match self {
            Self::RoValve => 0,
            Self::Booster => 1,
            Self::TapValve => 2,
            Self::Mixer => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::RoValve => "ro_valve",
            Self::Booster => "booster",
            Self::TapValve => "tap_valve",
            Self::Mixer => "mixer",
        }
    }

    pub const fn target(self) -> OverrideTarget {
        match self {
            Self::RoValve | Self::Booster => OverrideTarget::Ro,
            Self::TapValve => OverrideTarget::Tap,
            Self::Mixer => OverrideTarget::Mixer,
        }
    }
}

impl FromStr for ReservoirChannel {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|channel| channel.name() == value)
            .ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum Inlet {
    Ro,
    Tap,
}

impl Inlet {
    pub const ALL: [Self; 2] = [Self::Ro, Self::Tap];

    pub const fn index(self) -> usize {
        match self {
            Self::Ro => 0,
            Self::Tap => 1,
        }
    }

    pub const fn valve(self) -> ReservoirChannel {
        match self {
            Self::Ro => ReservoirChannel::RoValve,
            Self::Tap => ReservoirChannel::TapValve,
        }
    }

    pub const fn target(self) -> OverrideTarget {
        match self {
            Self::Ro => OverrideTarget::Ro,
            Self::Tap => OverrideTarget::Tap,
        }
    }

    pub const fn other(self) -> Self {
        match self {
            Self::Ro => Self::Tap,
            Self::Tap => Self::Ro,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideTarget {
    Ro,
    Tap,
    Mixer,
}

impl OverrideTarget {
    pub const ALL: [Self; 3] = [Self::Ro, Self::Tap, Self::Mixer];

    pub const fn index(self) -> usize {
        match self {
            Self::Ro => 0,
            Self::Tap => 1,
            Self::Mixer => 2,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Ro => "ro",
            Self::Tap => "tap",
            Self::Mixer => "mixer",
        }
    }

    pub const fn inlet(self) -> Option<Inlet> {
        match self {
            Self::Ro => Some(Inlet::Ro),
            Self::Tap => Some(Inlet::Tap),
            Self::Mixer => None,
        }
    }
}

impl FromStr for OverrideTarget {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|target| target.name() == value)
            .ok_or(())
    }
}

pub const MAX_OVERRIDE_S: u32 = 600;

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
