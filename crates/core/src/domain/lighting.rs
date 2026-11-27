use serde::{Deserialize, Serialize};

use super::metrics::{LightingMetricsDto, LightingProfile, LightingScheduleDto};

pub const MAX_CHANNEL_PCT: u8 = 100;

const RAMP_MINUTES: u16 = 60;
const HALF_RAMP_MINUTES: u16 = RAMP_MINUTES / 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct ChannelLevels {
    pub red_pct: u8,
    pub green_pct: u8,
    pub blue_pct: u8,
    pub white_pct: u8,
}

impl ChannelLevels {
    pub const OFF: Self = Self::new(0, 0, 0, 0);

    pub const fn new(red_pct: u8, green_pct: u8, blue_pct: u8, white_pct: u8) -> Self {
        Self {
            red_pct,
            green_pct,
            blue_pct,
            white_pct,
        }
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        let channels = [self.red_pct, self.green_pct, self.blue_pct, self.white_pct];

        if channels.iter().any(|&pct| pct > MAX_CHANNEL_PCT) {
            return Err("channel level out of range");
        }

        Ok(())
    }

    pub fn lerp(from: Self, to: Self, progress: f32) -> Self {
        let progress = progress.clamp(0.0, 1.0);
        let mix =
            |a: u8, b: u8| (f32::from(a) + (f32::from(b) - f32::from(a)) * progress).round() as u8;

        Self::new(
            mix(from.red_pct, to.red_pct),
            mix(from.green_pct, to.green_pct),
            mix(from.blue_pct, to.blue_pct),
            mix(from.white_pct, to.white_pct),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct LightingPalette {
    pub sunrise: ChannelLevels,
    pub daylight: ChannelLevels,
    pub sunset: ChannelLevels,
    pub moonlight: ChannelLevels,
}

impl LightingPalette {
    pub const DEFAULT: Self = Self {
        sunrise: ChannelLevels::new(40, 15, 0, 20),
        daylight: ChannelLevels::new(10, 10, 25, 60),
        sunset: ChannelLevels::new(45, 10, 5, 15),
        moonlight: ChannelLevels::new(4, 0, 10, 0),
    };

    pub fn validate(&self) -> Result<(), &'static str> {
        for levels in [self.sunrise, self.daylight, self.sunset, self.moonlight] {
            levels.validate()?;
        }

        Ok(())
    }
}

impl Default for LightingPalette {
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct LightingPalettePatch {
    #[serde(default)]
    pub sunrise: Option<ChannelLevels>,

    #[serde(default)]
    pub daylight: Option<ChannelLevels>,

    #[serde(default)]
    pub sunset: Option<ChannelLevels>,

    #[serde(default)]
    pub moonlight: Option<ChannelLevels>,
}

impl LightingPalettePatch {
    pub fn merge_onto(&self, base: LightingPalette) -> LightingPalette {
        LightingPalette {
            sunrise: self.sunrise.unwrap_or(base.sunrise),
            daylight: self.daylight.unwrap_or(base.daylight),
            sunset: self.sunset.unwrap_or(base.sunset),
            moonlight: self.moonlight.unwrap_or(base.moonlight),
        }
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
    pub fn levels(&self, palette: &LightingPalette) -> ChannelLevels {
        match self {
            Self::Profile(profile) => profile.levels(palette),
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
    pub const fn levels(self, palette: &LightingPalette) -> ChannelLevels {
        match self {
            Self::Off => ChannelLevels::OFF,
            Self::Sunrise => palette.sunrise,
            Self::Daylight => palette.daylight,
            Self::Sunset => palette.sunset,
            Self::Moonlight => palette.moonlight,
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

    pub fn evaluate(
        &self,
        current_minute: u16,
        current_month: u8,
        palette: &LightingPalette,
    ) -> LightingMetricsDto {
        let minute = current_minute % 1440;
        let (sunrise, sunset) = Self::get_solar_bounds(current_month);
        let daylight_start = sunrise + RAMP_MINUTES;
        let sunset_start = sunset.saturating_sub(RAMP_MINUTES);

        let (profile, channels) = if minute < sunrise || minute >= sunset {
            (LightingProfile::Off, ChannelLevels::OFF)
        } else if minute < daylight_start {
            (
                LightingProfile::Sunrise,
                ramp(
                    minute - sunrise,
                    ChannelLevels::OFF,
                    palette.sunrise,
                    palette.daylight,
                ),
            )
        } else if minute < sunset_start {
            (LightingProfile::Daylight, palette.daylight)
        } else {
            (
                LightingProfile::Sunset,
                ramp(
                    minute - sunset_start,
                    palette.daylight,
                    palette.sunset,
                    ChannelLevels::OFF,
                ),
            )
        };

        LightingMetricsDto::scheduled(profile, channels)
    }

    pub fn try_preview(
        &self,
        month: u8,
        step_minutes: u16,
        palette: &LightingPalette,
    ) -> Option<LightingScheduleDto> {
        if !(1..=12).contains(&month) {
            return None;
        }

        if step_minutes == 0 || 1440 % step_minutes != 0 {
            return None;
        }

        let samples = (1440 / step_minutes) as usize;
        let mut preview = LightingScheduleDto {
            month,
            step_minutes,
            red_pct: Vec::with_capacity(samples),
            green_pct: Vec::with_capacity(samples),
            blue_pct: Vec::with_capacity(samples),
            white_pct: Vec::with_capacity(samples),
        };

        for index in 0..samples {
            let channels = self
                .evaluate(index as u16 * step_minutes, month, palette)
                .channels;
            preview.red_pct.push(channels.red_pct);
            preview.green_pct.push(channels.green_pct);
            preview.blue_pct.push(channels.blue_pct);
            preview.white_pct.push(channels.white_pct);
        }

        Some(preview)
    }
}

impl Default for LightingSchedule {
    fn default() -> Self {
        Self::new()
    }
}

fn ramp(
    elapsed: u16,
    start: ChannelLevels,
    middle: ChannelLevels,
    end: ChannelLevels,
) -> ChannelLevels {
    if elapsed < HALF_RAMP_MINUTES {
        ChannelLevels::lerp(
            start,
            middle,
            f32::from(elapsed) / f32::from(HALF_RAMP_MINUTES),
        )
    } else {
        ChannelLevels::lerp(
            middle,
            end,
            f32::from(elapsed - HALF_RAMP_MINUTES) / f32::from(HALF_RAMP_MINUTES),
        )
    }
}

pub fn resolve(
    scheduled: LightingMetricsDto,
    over: Option<&LightingOverride>,
    palette: &LightingPalette,
) -> LightingMetricsDto {
    let Some(over) = over else {
        return scheduled;
    };

    LightingMetricsDto {
        profile: match over.target {
            LightingTarget::Profile(profile) => profile,
            LightingTarget::Channels(_) => scheduled.profile,
        },
        channels: over.target.levels(palette),
        override_active: true,
        available: true,
    }
}

#[cfg(test)]
mod tests;
