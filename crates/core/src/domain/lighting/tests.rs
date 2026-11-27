use super::*;
use crate::domain::metrics::LightingProfile;

const JUNE: u8 = 6;
const JUNE_SUNRISE: u16 = 4 * 60 + 30;
const JUNE_SUNSET: u16 = 20 * 60 + 50;

fn palette() -> LightingPalette {
    LightingPalette {
        sunrise: ChannelLevels::new(40, 20, 0, 20),
        daylight: ChannelLevels::new(10, 10, 30, 60),
        sunset: ChannelLevels::new(50, 10, 0, 10),
        moonlight: ChannelLevels::new(4, 0, 10, 0),
    }
}

fn at(minute: u16) -> LightingMetricsDto {
    LightingSchedule::new().evaluate(minute, JUNE, &palette())
}

#[test]
fn before_sunrise_is_off() {
    let m = at(JUNE_SUNRISE - 1);
    assert_eq!(m.profile, LightingProfile::Off);
    assert_eq!(m.channels, ChannelLevels::OFF);
    assert!(m.available);
    assert!(!m.override_active);
}

#[test]
fn sunrise_starts_from_zero() {
    let m = at(JUNE_SUNRISE);
    assert_eq!(m.profile, LightingProfile::Sunrise);
    assert_eq!(m.channels, ChannelLevels::OFF);
}

#[test]
fn sunrise_midpoint_reaches_the_sunrise_colour() {
    let m = at(JUNE_SUNRISE + 30);
    assert_eq!(m.profile, LightingProfile::Sunrise);
    assert_eq!(m.channels, palette().sunrise);
}

#[test]
fn first_half_of_sunrise_fades_in_the_sunrise_colour() {
    let m = at(JUNE_SUNRISE + 15);
    assert_eq!(m.channels, ChannelLevels::new(20, 10, 0, 10));
}

#[test]
fn second_half_of_sunrise_blends_into_daylight() {
    let m = at(JUNE_SUNRISE + 45);
    assert_eq!(m.channels, ChannelLevels::new(25, 15, 15, 40));
}

#[test]
fn daylight_uses_the_daylight_colour() {
    let m = at(JUNE_SUNRISE + 60);
    assert_eq!(m.profile, LightingProfile::Daylight);
    assert_eq!(m.channels, palette().daylight);
}

#[test]
fn sunset_midpoint_reaches_the_sunset_colour() {
    let m = at(JUNE_SUNSET - 30);
    assert_eq!(m.profile, LightingProfile::Sunset);
    assert_eq!(m.channels, palette().sunset);
}

#[test]
fn sunset_starts_from_daylight() {
    let m = at(JUNE_SUNSET - 60);
    assert_eq!(m.profile, LightingProfile::Sunset);
    assert_eq!(m.channels, palette().daylight);
}

#[test]
fn second_half_of_sunset_fades_out() {
    let m = at(JUNE_SUNSET - 15);
    assert_eq!(m.channels, ChannelLevels::new(25, 5, 0, 5));
}

#[test]
fn at_sunset_minute_lights_are_off() {
    let m = at(JUNE_SUNSET);
    assert_eq!(m.profile, LightingProfile::Off);
    assert_eq!(m.channels, ChannelLevels::OFF);
}

#[test]
fn minute_wraps_past_midnight() {
    assert_eq!(at(1440 + 600), at(600));
}

#[test]
fn invalid_month_falls_back_to_default_bounds() {
    let schedule = LightingSchedule::new();
    let off = schedule.evaluate(419, 0, &palette());
    let on = schedule.evaluate(421, 0, &palette());
    assert_eq!(off.profile, LightingProfile::Off);
    assert_eq!(on.profile, LightingProfile::Sunrise);
}

#[test]
fn every_month_has_sunrise_before_sunset() {
    for month in 1..=12 {
        let (sunrise, sunset) = LightingSchedule::get_solar_bounds(month);
        assert!(sunrise < sunset, "month {month}: {sunrise} !< {sunset}");
        assert!(
            sunset - sunrise >= 2 * RAMP_MINUTES,
            "month {month}: window shorter than both ramps"
        );
    }
}

#[test]
fn every_minute_of_every_month_follows_the_daily_profile_order() {
    for month in 1..=12 {
        let mut order: Vec<LightingProfile> = Vec::new();
        for minute in 0..1440 {
            let profile = LightingSchedule::new()
                .evaluate(minute, month, &palette())
                .profile;
            if order.last() != Some(&profile) {
                order.push(profile);
            }
        }
        assert_eq!(
            order,
            vec![
                LightingProfile::Off,
                LightingProfile::Sunrise,
                LightingProfile::Daylight,
                LightingProfile::Sunset,
                LightingProfile::Off,
            ],
            "month {month}"
        );
    }
}

#[test]
fn channel_levels_accept_the_full_legal_range() {
    assert!(ChannelLevels::OFF.validate().is_ok());
    assert!(ChannelLevels::new(100, 100, 100, 100).validate().is_ok());
}

#[test]
fn channel_levels_reject_any_channel_above_one_hundred() {
    assert!(ChannelLevels::new(101, 0, 0, 0).validate().is_err());
    assert!(ChannelLevels::new(0, 101, 0, 0).validate().is_err());
    assert!(ChannelLevels::new(0, 0, 101, 0).validate().is_err());
    assert!(ChannelLevels::new(0, 0, 0, 101).validate().is_err());
}

#[test]
fn lerp_hits_both_ends_and_rounds_between() {
    let from = ChannelLevels::new(0, 10, 100, 1);
    let to = ChannelLevels::new(100, 0, 0, 2);
    assert_eq!(ChannelLevels::lerp(from, to, 0.0), from);
    assert_eq!(ChannelLevels::lerp(from, to, 1.0), to);
    assert_eq!(
        ChannelLevels::lerp(from, to, 0.5),
        ChannelLevels::new(50, 5, 50, 2)
    );
}

#[test]
fn lerp_clamps_progress() {
    let from = ChannelLevels::OFF;
    let to = ChannelLevels::new(100, 100, 100, 100);
    assert_eq!(ChannelLevels::lerp(from, to, -1.0), from);
    assert_eq!(ChannelLevels::lerp(from, to, 2.0), to);
}

#[test]
fn profiles_take_their_levels_from_the_palette() {
    let p = palette();
    assert_eq!(LightingProfile::Off.levels(&p), ChannelLevels::OFF);
    assert_eq!(LightingProfile::Sunrise.levels(&p), p.sunrise);
    assert_eq!(LightingProfile::Daylight.levels(&p), p.daylight);
    assert_eq!(LightingProfile::Sunset.levels(&p), p.sunset);
    assert_eq!(LightingProfile::Moonlight.levels(&p), p.moonlight);
}

#[test]
fn the_default_palette_is_valid() {
    assert!(LightingPalette::DEFAULT.validate().is_ok());
}

#[test]
fn a_palette_with_one_bad_colour_is_rejected() {
    let mut p = palette();
    p.moonlight.blue_pct = 101;
    assert!(p.validate().is_err());
}

#[test]
fn a_patch_replaces_only_the_colours_it_carries() {
    let patch = LightingPalettePatch {
        sunset: Some(ChannelLevels::new(1, 2, 3, 4)),
        ..LightingPalettePatch::default()
    };
    let merged = patch.merge_onto(palette());
    assert_eq!(merged.sunset, ChannelLevels::new(1, 2, 3, 4));
    assert_eq!(merged.sunrise, palette().sunrise);
    assert_eq!(merged.daylight, palette().daylight);
    assert_eq!(merged.moonlight, palette().moonlight);
}

#[test]
fn a_profile_override_uses_the_palette_and_names_the_profile() {
    let over = LightingOverride {
        target: LightingTarget::Profile(LightingProfile::Moonlight),
        expires_in_s: None,
    };
    let m = resolve(at(600), Some(&over), &palette());
    assert_eq!(m.profile, LightingProfile::Moonlight);
    assert_eq!(m.channels, palette().moonlight);
    assert!(m.override_active);
}

#[test]
fn a_channel_override_keeps_the_scheduled_profile_name() {
    let over = LightingOverride {
        target: LightingTarget::Channels(ChannelLevels::new(1, 2, 3, 4)),
        expires_in_s: None,
    };
    let m = resolve(at(600), Some(&over), &palette());
    assert_eq!(m.profile, LightingProfile::Daylight);
    assert_eq!(m.channels, ChannelLevels::new(1, 2, 3, 4));
    assert!(m.override_active);
}

#[test]
fn no_override_returns_the_schedule() {
    assert_eq!(resolve(at(600), None, &palette()), at(600));
}

#[test]
fn preview_has_one_sample_per_step_for_every_channel() {
    let preview = LightingSchedule::new()
        .try_preview(JUNE, 5, &palette())
        .expect("valid month and step");
    assert_eq!(preview.red_pct.len(), 288);
    assert_eq!(preview.green_pct.len(), 288);
    assert_eq!(preview.blue_pct.len(), 288);
    assert_eq!(preview.white_pct.len(), 288);
    assert_eq!(preview.white_pct[120], palette().daylight.white_pct);
}

#[test]
fn preview_rejects_a_bad_month_or_step() {
    let schedule = LightingSchedule::new();
    assert!(schedule.try_preview(13, 5, &palette()).is_none());
    assert!(schedule.try_preview(6, 0, &palette()).is_none());
    assert!(schedule.try_preview(6, 7, &palette()).is_none());
}
