use super::metrics::{LightingMetricsDto, LightingProfile};

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
            }
        } else if current_minute >= sunrise_min && current_minute < daylight_start_min {
            let elapsed = current_minute - sunrise_min;
            let progress = (elapsed as f32 / 60.0).clamp(0.0, 1.0);

            LightingMetricsDto {
                profile: LightingProfile::Sunrise,
                channel_white_pct: (progress * 60.0) as u8,
                channel_rgb_pct: (progress * 40.0) as u8,
            }
        } else if current_minute >= daylight_start_min && current_minute < sunset_start_min {
            LightingMetricsDto {
                profile: LightingProfile::Daylight,
                channel_white_pct: 60,
                channel_rgb_pct: 40,
            }
        } else {
            let elapsed = current_minute - sunset_start_min;
            let progress = 1.0 - (elapsed as f32 / 60.0).clamp(0.0, 1.0);

            LightingMetricsDto {
                profile: LightingProfile::Sunset,
                channel_white_pct: (progress * 60.0) as u8,
                channel_rgb_pct: (progress * 40.0) as u8,
            }
        }
    }
}

impl Default for LightingSchedule {
    fn default() -> Self {
        Self::new()
    }
}
