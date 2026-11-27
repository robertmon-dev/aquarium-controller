#[cfg(target_os = "espidf")]
pub use hw::{off_channel, timer, LedcBank};

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::OutputPin;
    use esp_idf_hal::ledc::{
        config::TimerConfig, LedcChannel, LedcDriver, LedcTimer, LedcTimerDriver, Resolution,
        SpeedMode,
    };
    use esp_idf_hal::sys::EspError;
    use esp_idf_hal::units::FromValueType;

    use aquarium_core::protocol::pwm::duty_for_pct;

    pub fn timer<'d, T: LedcTimer + 'd>(
        timer: T,
        freq_hz: u32,
    ) -> Result<LedcTimerDriver<'d, T::SpeedMode>, EspError> {
        let config = TimerConfig::new()
            .frequency(freq_hz.Hz())
            .resolution(Resolution::Bits12);
        LedcTimerDriver::new(timer, &config)
    }

    pub fn off_channel<'d, S: SpeedMode>(
        channel: impl LedcChannel<SpeedMode = S> + 'd,
        timer: &'d LedcTimerDriver<'d, S>,
        pin: impl OutputPin + 'd,
    ) -> Result<LedcDriver<'d>, EspError> {
        let mut driver = LedcDriver::new(channel, timer, pin)?;
        driver.set_duty(0)?;
        Ok(driver)
    }

    pub struct LedcBank<'d, const N: usize> {
        channels: [LedcDriver<'d>; N],
    }

    impl<'d, const N: usize> LedcBank<'d, N> {
        pub fn new(channels: [LedcDriver<'d>; N]) -> Self {
            Self { channels }
        }

        pub fn set_pct(&mut self, index: usize, pct: u8) -> Result<(), &'static str> {
            let driver = &mut self.channels[index];
            let duty = duty_for_pct(pct, driver.get_max_duty());
            driver
                .set_duty(duty)
                .map_err(|_| "the PWM output could not be driven")
        }
    }
}
