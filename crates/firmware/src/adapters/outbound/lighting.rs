pub const fn duty_for_pct(pct: u8, max_duty: u32) -> u32 {
    let clamped = if pct > 100 { 100 } else { pct } as u32;
    (clamped * max_duty + 50) / 100
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightChannel {
    White,
    Rgb,
}

impl LightChannel {
    pub const fn index(self) -> usize {
        match self {
            Self::White => 0,
            Self::Rgb => 1,
        }
    }
}

#[cfg(target_os = "espidf")]
pub use hw::LedMosfetOutputs;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::OutputPin;
    use esp_idf_hal::ledc::{
        config::TimerConfig, LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver, Resolution,
        SpeedMode,
    };
    use esp_idf_hal::sys::EspError;
    use esp_idf_hal::units::FromValueType;

    use super::{duty_for_pct, LightChannel};

    use aquarium_core::domain::lighting::ChannelLevels;
    use aquarium_core::ports::outbound::LightingOutputs;

    pub struct LedMosfetOutputs<'d> {
        channels: [LedcDriver<'d>; 2],
    }

    fn off_channel<'d, S: SpeedMode>(
        channel: impl LedcChannel<SpeedMode = S> + 'd,
        timer: &'d LedcTimerDriver<'d, S>,
        pin: impl OutputPin + 'd,
    ) -> Result<LedcDriver<'d>, EspError> {
        let mut driver = LedcDriver::new(channel, timer, pin)?;
        driver.set_duty(0)?;
        Ok(driver)
    }

    impl<'d> LedMosfetOutputs<'d> {
        pub fn create_timer<T: LedcTimer + 'd>(
            timer: T,
            freq_hz: u32,
        ) -> Result<LedcTimerDriver<'d, T::SpeedMode>, EspError> {
            let config = TimerConfig::new()
                .frequency(freq_hz.Hz())
                .resolution(Resolution::Bits12);
            LedcTimerDriver::new(timer, &config)
        }

        pub fn new<S: SpeedMode>(
            timer: &'d LedcTimerDriver<'d, S>,
            white_ch: impl LedcChannel<SpeedMode = S> + 'd,
            white_pin: impl OutputPin + 'd,
            rgb_ch: impl LedcChannel<SpeedMode = S> + 'd,
            rgb_pin: impl OutputPin + 'd,
        ) -> Result<Self, EspError> {
            Ok(Self {
                channels: [
                    off_channel(white_ch, timer, white_pin)?,
                    off_channel(rgb_ch, timer, rgb_pin)?,
                ],
            })
        }

        pub fn set_pct(&mut self, channel: LightChannel, pct: u8) -> Result<(), &'static str> {
            let driver = &mut self.channels[channel.index()];
            let duty = duty_for_pct(pct, driver.get_max_duty());
            driver
                .set_duty(duty)
                .map_err(|_| "the LED PWM output could not be driven")
        }
    }

    impl LightingOutputs for LedMosfetOutputs<'_> {
        fn set_levels(&mut self, levels: ChannelLevels) -> Result<(), &'static str> {
            self.set_pct(LightChannel::White, levels.white_pct)?;
            self.set_pct(LightChannel::Rgb, levels.rgb_pct)?;
            Ok(())
        }
    }
}
