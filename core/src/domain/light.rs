use serde::{Deserialize, Serialize};

use super::metrics::{LightingMetricsDto, LightingProfile};

pub const DAYLIGHT_WHITE_PCT: u8 = 60;
pub const DAYLIGHT_RGB_PCT: u8 = 40;

pub const MOONLIGHT_WHITE_PCT: u8 = 0;
pub const MOONLIGHT_RGB_PCT: u8 = 5;

pub const MAX_CHANNEL_PCT: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
mod tests {
    use super::*;
    use crate::domain::metrics::LightingProfile;

    const JUNE: u8 = 6;

    #[test]
    fn before_sunrise_is_off() {
        let m = LightingSchedule::new().evaluate(269, JUNE);
        assert_eq!(m.profile, LightingProfile::Off);
        assert_eq!(m.channel_white_pct, 0);
        assert_eq!(m.channel_rgb_pct, 0);
    }

    #[test]
    fn sunrise_ramps_from_zero() {
        let m = LightingSchedule::new().evaluate(270, JUNE);
        assert_eq!(m.profile, LightingProfile::Sunrise);
        assert_eq!(m.channel_white_pct, 0);
    }

    #[test]
    fn sunrise_midpoint_is_half_brightness() {
        let m = LightingSchedule::new().evaluate(300, JUNE);
        assert_eq!(m.profile, LightingProfile::Sunrise);
        assert_eq!(m.channel_white_pct, 30);
        assert_eq!(m.channel_rgb_pct, 20);
    }

    #[test]
    fn daylight_is_full_declared_output() {
        let m = LightingSchedule::new().evaluate(600, JUNE);
        assert_eq!(m.profile, LightingProfile::Daylight);
        assert_eq!(m.channel_white_pct, 60);
        assert_eq!(m.channel_rgb_pct, 40);
    }

    #[test]
    fn sunset_window_dims_toward_night() {
        let early = LightingSchedule::new().evaluate(1191, JUNE);
        let late = LightingSchedule::new().evaluate(1240, JUNE);
        assert_eq!(early.profile, LightingProfile::Sunset);
        assert_eq!(late.profile, LightingProfile::Sunset);
        assert!(late.channel_white_pct < early.channel_white_pct);
    }

    #[test]
    fn at_sunset_minute_lights_are_off() {
        let m = LightingSchedule::new().evaluate(1250, JUNE);
        assert_eq!(m.profile, LightingProfile::Off);
    }

    #[test]
    fn minute_wraps_past_midnight() {
        let wrapped = LightingSchedule::new().evaluate(1440 + 600, JUNE);
        let direct = LightingSchedule::new().evaluate(600, JUNE);
        assert_eq!(wrapped.profile, direct.profile);
        assert_eq!(wrapped.channel_white_pct, direct.channel_white_pct);
    }

    #[test]
    fn invalid_month_falls_back_to_default_bounds() {
        let off = LightingSchedule::new().evaluate(419, 0);
        let on = LightingSchedule::new().evaluate(421, 0);
        assert_eq!(off.profile, LightingProfile::Off);
        assert_eq!(on.profile, LightingProfile::Sunrise);
    }

    #[test]
    fn every_month_has_sunrise_before_sunset() {
        for month in 1..=12 {
            let (sunrise, sunset) = LightingSchedule::get_solar_bounds(month);
            assert!(sunrise < sunset, "month {month}: {sunrise} !< {sunset}");
            assert!(
                sunset - sunrise > 120,
                "month {month}: okno krótsze niż 2 h"
            );
        }
    }

    #[test]
    fn channel_levels_accept_the_full_legal_range() {
        assert!(ChannelLevels::new(0, 0).validate().is_ok());
        assert!(ChannelLevels::new(100, 100).validate().is_ok());
        assert!(ChannelLevels::new(60, 40).validate().is_ok());
    }

    #[test]
    fn channel_levels_reject_anything_above_one_hundred() {
        assert!(ChannelLevels::new(101, 0).validate().is_err());
        assert!(ChannelLevels::new(0, 101).validate().is_err());
        assert!(ChannelLevels::new(255, 255).validate().is_err());
    }

    #[test]
    fn every_profile_maps_to_concrete_levels() {
        assert_eq!(
            LightingProfile::Off.static_levels(),
            ChannelLevels::new(0, 0)
        );
        assert_eq!(
            LightingProfile::Daylight.static_levels(),
            ChannelLevels::new(DAYLIGHT_WHITE_PCT, DAYLIGHT_RGB_PCT)
        );
        assert_eq!(
            LightingProfile::Moonlight.static_levels(),
            ChannelLevels::new(MOONLIGHT_WHITE_PCT, MOONLIGHT_RGB_PCT)
        );
    }

    #[test]
    fn ramps_map_to_their_endpoint_which_equals_daylight() {
        assert_eq!(
            LightingProfile::Sunrise.static_levels(),
            LightingProfile::Daylight.static_levels(),
            "sunrise as a static target is its endpoint"
        );
        assert_eq!(
            LightingProfile::Sunset.static_levels(),
            LightingProfile::Daylight.static_levels()
        );
    }

    #[test]
    fn target_levels_follow_the_variant() {
        assert_eq!(
            LightingTarget::Profile(LightingProfile::Off).levels(),
            ChannelLevels::new(0, 0)
        );
        assert_eq!(
            LightingTarget::Channels(ChannelLevels::new(80, 20)).levels(),
            ChannelLevels::new(80, 20)
        );
    }

    #[test]
    fn a_profile_target_is_always_valid_but_channels_are_checked() {
        assert!(LightingTarget::Profile(LightingProfile::Moonlight)
            .validate()
            .is_ok());
        assert!(LightingTarget::Channels(ChannelLevels::new(100, 100))
            .validate()
            .is_ok());
        assert!(LightingTarget::Channels(ChannelLevels::new(101, 0))
            .validate()
            .is_err());
    }

    fn scheduled_daylight() -> LightingMetricsDto {
        LightingSchedule::new().evaluate(600, JUNE)
    }

    #[test]
    fn without_an_override_the_schedule_passes_through_untouched() {
        let scheduled = scheduled_daylight();
        let resolved = resolve(scheduled.clone(), None);
        assert_eq!(resolved, scheduled);
        assert!(!resolved.override_active);
    }

    #[test]
    fn a_profile_override_replaces_profile_and_levels() {
        let over = LightingOverride {
            target: LightingTarget::Profile(LightingProfile::Off),
            expires_in_s: None,
        };

        let resolved = resolve(scheduled_daylight(), Some(&over));

        assert_eq!(resolved.profile, LightingProfile::Off);
        assert_eq!(resolved.channel_white_pct, 0);
        assert_eq!(resolved.channel_rgb_pct, 0);
        assert!(resolved.override_active);
    }

    #[test]
    fn a_channels_override_keeps_the_scheduled_profile() {
        let over = LightingOverride {
            target: LightingTarget::Channels(ChannelLevels::new(80, 20)),
            expires_in_s: None,
        };

        let resolved = resolve(scheduled_daylight(), Some(&over));

        assert_eq!(
            resolved.profile,
            LightingProfile::Daylight,
            "raw channels match no profile, so the scheduled profile stays"
        );
        assert_eq!(resolved.channel_white_pct, 80);
        assert_eq!(resolved.channel_rgb_pct, 20);
        assert!(
            resolved.override_active,
            "override_active is the ONLY signal that the schedule is not in charge"
        );
    }

    #[test]
    fn an_override_wins_even_when_the_schedule_says_off() {
        let night = LightingSchedule::new().evaluate(60, JUNE);
        assert_eq!(night.profile, LightingProfile::Off);

        let over = LightingOverride {
            target: LightingTarget::Profile(LightingProfile::Moonlight),
            expires_in_s: Some(120),
        };

        let resolved = resolve(night, Some(&over));

        assert_eq!(resolved.profile, LightingProfile::Moonlight);
        assert_eq!(resolved.channel_rgb_pct, MOONLIGHT_RGB_PCT);
        assert!(resolved.override_active);
    }
}
