#[cfg(target_os = "espidf")]
pub use hw::SumpLightOutput;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{Output, OutputPin, PinDriver};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::ports::outbound::SumpLightOutputs;

    pub struct SumpLightOutput<'d>(PinDriver<'d, Output>);

    impl<'d> SumpLightOutput<'d> {
        pub fn new(pin: impl OutputPin + 'd) -> Result<Self, EspError> {
            let mut driver = PinDriver::output(pin)?;
            driver.set_low()?;
            Ok(Self(driver))
        }
    }

    impl SumpLightOutputs for SumpLightOutput<'_> {
        fn set(&mut self, on: bool) -> Result<(), &'static str> {
            let result = if on {
                self.0.set_high()
            } else {
                self.0.set_low()
            };

            result.map_err(|_| "the sump light output could not be driven")
        }
    }
}
