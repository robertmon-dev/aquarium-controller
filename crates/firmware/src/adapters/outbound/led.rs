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
    };
    use esp_idf_hal::sys::EspError;
    use esp_idf_hal::units::FromValueType;

    use super::{duty_for_pct, LightChannel};

    pub struct LedMosfetOutputs<'d> {
        channels: [LedcDriver<'d>; 2],
    }

    fn off_channel<'d>(
        channel: impl LedcChannel + 'd,
        timer: &'d LedcTimerDriver<'d>,
        pin: impl OutputPin + 'd,
    ) -> Result<LedcDriver<'d>, EspError> {
        let mut driver = LedcDriver::new(channel, timer, pin)?;
        driver.set_duty(0)?;
        Ok(driver)
    }

    impl<'d> LedMosfetOutputs<'d> {
        pub fn create_timer(
            timer: impl LedcTimer + 'd,
            freq_hz: u32,
        ) -> Result<LedcTimerDriver<'d>, EspError> {
            let config = TimerConfig::new()
                .frequency(freq_hz.Hz())
                .resolution(Resolution::Bits12);
            LedcTimerDriver::new(timer, &config)
        }

        pub fn new(
            timer: &'d LedcTimerDriver<'d>,
            white_ch: impl LedcChannel + 'd,
            white_pin: impl OutputPin + 'd,
            rgb_ch: impl LedcChannel + 'd,
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
}
