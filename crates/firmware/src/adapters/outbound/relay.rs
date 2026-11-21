#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Contact {
    NormallyOpen,
    NormallyClosed,
}

impl Contact {
    pub const fn coil_for(self, device_on: bool) -> bool {
        match self {
            Self::NormallyOpen => device_on,
            Self::NormallyClosed => !device_on,
        }
    }
}

#[cfg(target_os = "espidf")]
pub use hw::RelayOutputs;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{Output, OutputPin, PinDriver};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::domain::pump::PumpId;
    use aquarium_core::ports::outbound::PumpOutputs;

    use super::Contact;

    pub struct RelayOutputs<'d> {
        pins: [PinDriver<'d, Output>; 4],
        contacts: [Contact; 4],
    }

    fn released<'d>(pin: impl OutputPin + 'd) -> Result<PinDriver<'d, Output>, EspError> {
        let mut driver = PinDriver::output(pin)?;
        driver.set_low()?;
        Ok(driver)
    }

    impl<'d> RelayOutputs<'d> {
        pub fn new(
            return_pump: impl OutputPin + 'd,
            uv: impl OutputPin + 'd,
            makeup: impl OutputPin + 'd,
            drain: impl OutputPin + 'd,
            contacts: [Contact; 4],
        ) -> Result<Self, EspError> {
            Ok(Self {
                pins: [
                    released(return_pump)?,
                    released(uv)?,
                    released(makeup)?,
                    released(drain)?,
                ],
                contacts,
            })
        }

        pub fn set_coil(&mut self, id: PumpId, energized: bool) -> Result<(), &'static str> {
            let pin = &mut self.pins[id.index()];
            let result = if energized {
                pin.set_high()
            } else {
                pin.set_low()
            };

            result.map_err(|_| "the relay input could not be driven")
        }
    }

    impl PumpOutputs for RelayOutputs<'_> {
        fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
            let coil = self.contacts[id.index()].coil_for(on);
            self.set_coil(id, coil)
        }
    }
}
