#[cfg(target_os = "espidf")]
pub use hw::SsrOutputs;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{Output, OutputPin, PinDriver};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::domain::heater::HeaterId;
    use aquarium_core::ports::outbound::HeaterOutputs;

    pub struct SsrOutputs<'d> {
        pins: [PinDriver<'d, Output>; 2],
    }

    impl<'d> SsrOutputs<'d> {
        pub fn new(tank: impl OutputPin + 'd, sump: impl OutputPin + 'd) -> Result<Self, EspError> {
            let mut tank = PinDriver::output(tank)?;
            tank.set_low()?;

            let mut sump = PinDriver::output(sump)?;
            sump.set_low()?;

            Ok(Self { pins: [tank, sump] })
        }
    }

    impl HeaterOutputs for SsrOutputs<'_> {
        fn set(&mut self, id: HeaterId, on: bool) -> Result<(), &'static str> {
            let pin = &mut self.pins[id.index()];
            let result = if on { pin.set_high() } else { pin.set_low() };

            result.map_err(|_| "the SSR pin could not be driven")
        }
    }
}
