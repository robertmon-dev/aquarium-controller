use core::str::FromStr;

use serde::{Deserialize, Serialize};

pub const MIN_RETURN_OVERRIDE_S: u32 = 60;
pub const MAX_RETURN_OVERRIDE_S: u32 = 7200;
pub const MIN_UV_OVERRIDE_S: u32 = 60;
pub const MAX_UV_OVERRIDE_S: u32 = 2_592_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum PumpId {
    Return,
    Uv,
    Makeup,
    Drain,
}

impl PumpId {
    pub const ALL: [Self; 4] = [Self::Return, Self::Uv, Self::Makeup, Self::Drain];

    pub const fn index(self) -> usize {
        match self {
            Self::Return => 0,
            Self::Uv => 1,
            Self::Makeup => 2,
            Self::Drain => 3,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Return => "return",
            Self::Uv => "uv",
            Self::Makeup => "makeup",
            Self::Drain => "drain",
        }
    }

    pub const fn fail_safe_on(self) -> bool {
        matches!(self, Self::Return | Self::Uv)
    }

    pub const fn override_slot(self) -> Option<usize> {
        match self {
            Self::Return => Some(0),
            Self::Uv => Some(1),
            Self::Makeup | Self::Drain => None,
        }
    }
}

impl FromStr for PumpId {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|id| id.name() == value)
            .ok_or(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverrideError {
    NotOverridable,
    Invalid(&'static str),
    NotPersisted(&'static str),
}

pub fn validate_override(id: PumpId, duration_s: Option<u32>) -> Result<(), OverrideError> {
    match (id, duration_s) {
        (PumpId::Makeup | PumpId::Drain, _) => Err(OverrideError::NotOverridable),
        (PumpId::Return, None) => Err(OverrideError::Invalid(
            "a return pump override needs duration_s",
        )),
        (PumpId::Return, Some(seconds))
            if !(MIN_RETURN_OVERRIDE_S..=MAX_RETURN_OVERRIDE_S).contains(&seconds) =>
        {
            Err(OverrideError::Invalid(
                "a return pump override lasts 60 to 7200 s",
            ))
        }
        (PumpId::Uv, Some(seconds))
            if !(MIN_UV_OVERRIDE_S..=MAX_UV_OVERRIDE_S).contains(&seconds) =>
        {
            Err(OverrideError::Invalid(
                "a UV override lasts 60 s to 30 days",
            ))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
