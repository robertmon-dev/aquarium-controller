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
            "month {month}: window shorter than 2 h"
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

const DECEMBER: u8 = 12;
const STEP: u16 = 5;

#[test]
fn a_preview_covers_a_whole_day_at_the_requested_step() {
    let preview = LightingSchedule::new().preview(JUNE, STEP);

    assert_eq!(preview.month, JUNE);
    assert_eq!(preview.step_minutes, STEP);
    assert_eq!(preview.white_pct.len(), (1440 / STEP) as usize);
    assert_eq!(preview.rgb_pct.len(), (1440 / STEP) as usize);
}

#[test]
fn a_preview_agrees_with_evaluate_at_every_sample() {
    let schedule = LightingSchedule::new();
    let preview = schedule.preview(JUNE, STEP);

    for (index, white) in preview.white_pct.iter().enumerate() {
        let minute = index as u16 * STEP;
        let expected = schedule.evaluate(minute, JUNE);

        assert_eq!(
            *white, expected.channel_white_pct,
            "white disagrees at minute {minute}"
        );
        assert_eq!(
            preview.rgb_pct[index], expected.channel_rgb_pct,
            "rgb disagrees at minute {minute}"
        );
    }
}

#[test]
fn a_summer_preview_is_lit_for_longer_than_a_winter_one() {
    let schedule = LightingSchedule::new();

    let lit = |month: u8| {
        schedule
            .preview(month, STEP)
            .white_pct
            .iter()
            .filter(|pct| **pct > 0)
            .count()
    };

    assert!(
        lit(JUNE) > lit(DECEMBER),
        "june should be lit longer than december: {} vs {}",
        lit(JUNE),
        lit(DECEMBER)
    );
}

#[test]
fn a_step_that_does_not_divide_the_day_is_refused() {
    assert!(LightingSchedule::new().try_preview(JUNE, 7).is_none());
    assert!(LightingSchedule::new().try_preview(JUNE, 0).is_none());
}

#[test]
fn a_month_outside_the_calendar_is_refused() {
    assert!(LightingSchedule::new().try_preview(0, STEP).is_none());
    assert!(LightingSchedule::new().try_preview(13, STEP).is_none());
}
