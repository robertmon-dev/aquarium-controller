use serde::{Deserialize, Serialize};

use super::metrics::{LightingMetricsDto, LightingProfile};

pub const DAYLIGHT_WHITE_PCT: u8 = 60;
pub const DAYLIGHT_RGB_PCT: u8 = 40;

pub const MOONLIGHT_WHITE_PCT: u8 = 0;
pub const MOONLIGHT_RGB_PCT: u8 = 5;

pub const MAX_CHANNEL_PCT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ChannelLevels {
    pub white_pct: u8,
    pub rgb_pct: u8,
}

impl ChannelLevels {
    pub const fn new(white_pct: u8, rgb_pct: u8) -> Self {
        Self { white_pct, rgb_pct }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.white_pct > MAX_CHANNEL_PCT || self.rgb_pct > MAX_CHANNEL_PCT {
            return Err("channel level out of range");
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum LightingTarget {
    Profile(LightingProfile),
    Channels(ChannelLevels),
}

impl LightingTarget {
    pub fn levels(&self) -> ChannelLevels {
        match self {
            Self::Profile(profile) => profile.static_levels(),
            Self::Channels(levels) => *levels,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::Profile(_) => Ok(()),
            Self::Channels(levels) => levels.validate(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct LightingOverride {
    pub target: LightingTarget,
    pub expires_in_s: Option<u32>,
}

impl LightingProfile {
    pub const fn static_levels(self) -> ChannelLevels {
        match self {
            Self::Off => ChannelLevels::new(0, 0),
            Self::Sunrise | Self::Daylight | Self::Sunset => {
                ChannelLevels::new(DAYLIGHT_WHITE_PCT, DAYLIGHT_RGB_PCT)
            }
            Self::Moonlight => ChannelLevels::new(MOONLIGHT_WHITE_PCT, MOONLIGHT_RGB_PCT),
        }
    }
}

pub struct LightingSchedule;

impl LightingSchedule {
    pub const fn new() -> Self {
        Self
    }

    fn get_solar_bounds(month: u8) -> (u16, u16) {
        match month {
            1 => (7 * 60 + 45, 16 * 60 + 10),
            2 => (7 * 60 + 15, 16 * 60 + 55),
            3 => (6 * 60 + 20, 17 * 60 + 45),
            4 => (5 * 60 + 20, 19 * 60 + 30),
            5 => (4 * 60 + 40, 20 * 60 + 15),
            6 => (4 * 60 + 30, 20 * 60 + 50),
            7 => (4 * 60 + 50, 20 * 60 + 40),
            8 => (5 * 60 + 30, 19 * 60 + 50),
            9 => (6 * 60 + 15, 18 * 60 + 55),
            10 => (7 * 60, 17 * 60 + 55),
            11 => (6 * 60 + 50, 16 * 60 + 15),
            12 => (7 * 60 + 35, 16 * 60),
            _ => (7 * 60, 19 * 60),
        }
    }

    pub fn evaluate(&self, current_minute: u16, current_month: u8) -> LightingMetricsDto {
        let current_minute = current_minute % 1440;
        let (sunrise_min, sunset_min) = Self::get_solar_bounds(current_month);

        let daylight_start_min = sunrise_min + 60;
        let sunset_start_min = sunset_min.saturating_sub(60);
        let night_start_min = sunset_min;

        if current_minute < sunrise_min || current_minute >= night_start_min {
            LightingMetricsDto {
                profile: LightingProfile::Off,
                channel_white_pct: 0,
                channel_rgb_pct: 0,
                override_active: false,
            }
        } else if current_minute >= sunrise_min && current_minute < daylight_start_min {
            let elapsed = current_minute - sunrise_min;
            let progress = (elapsed as f32 / 60.0).clamp(0.0, 1.0);

            LightingMetricsDto {
                profile: LightingProfile::Sunrise,
                channel_white_pct: (progress * DAYLIGHT_WHITE_PCT as f32) as u8,
                channel_rgb_pct: (progress * DAYLIGHT_RGB_PCT as f32) as u8,
                override_active: false,
            }
        } else if current_minute >= daylight_start_min && current_minute < sunset_start_min {
            LightingMetricsDto {
                profile: LightingProfile::Daylight,
                channel_white_pct: DAYLIGHT_WHITE_PCT,
                channel_rgb_pct: DAYLIGHT_RGB_PCT,
                override_active: false,
            }
        } else {
            let elapsed = current_minute - sunset_start_min;
            let progress = 1.0 - (elapsed as f32 / 60.0).clamp(0.0, 1.0);

            LightingMetricsDto {
                profile: LightingProfile::Sunset,
                channel_white_pct: (progress * DAYLIGHT_WHITE_PCT as f32) as u8,
                channel_rgb_pct: (progress * DAYLIGHT_RGB_PCT as f32) as u8,
                override_active: false,
            }
        }
    }
}

pub fn resolve(
    scheduled: LightingMetricsDto,
    over: Option<&LightingOverride>,
) -> LightingMetricsDto {
    let Some(over) = over else {
        return scheduled;
    };

    let levels = over.target.levels();

    LightingMetricsDto {
        profile: match over.target {
            LightingTarget::Profile(profile) => profile,
            LightingTarget::Channels(_) => scheduled.profile,
        },
        channel_white_pct: levels.white_pct,
        channel_rgb_pct: levels.rgb_pct,
        override_active: true,
    }
}

impl Default for LightingSchedule {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests;
