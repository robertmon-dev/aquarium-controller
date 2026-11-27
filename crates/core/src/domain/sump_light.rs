use serde::{Deserialize, Serialize};

pub const MIN_DURATION_S: u32 = 60;
pub const MAX_DURATION_S: u32 = 86_400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct SumpLightStatus {
    pub on: bool,
    pub expires_in_s: Option<u32>,
    pub available: bool,
}

impl SumpLightStatus {
    pub const UNAVAILABLE: Self = Self {
        on: false,
        expires_in_s: None,
        available: false,
    };
}

impl Default for SumpLightStatus {
    fn default() -> Self {
        Self::UNAVAILABLE
    }
}

pub fn validate_request(on: bool, duration_s: Option<u32>) -> Result<(), &'static str> {
    match (on, duration_s) {
        (false, Some(_)) => Err("an OFF request takes no duration_s"),
        (true, Some(seconds)) if !(MIN_DURATION_S..=MAX_DURATION_S).contains(&seconds) => {
            Err("duration_s must be between 60 and 86400")
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests;
