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
pub use hw::{RelayOutputs, ReservoirRelayOutputs};

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{Output, OutputPin, PinDriver};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::domain::pump::PumpId;
    use aquarium_core::domain::reservoir::ReservoirChannel;
    use aquarium_core::ports::outbound::{PumpOutputs, ReservoirOutputs};

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
            self.drive(id.index(), energized)
        }

        fn drive(&mut self, index: usize, energized: bool) -> Result<(), &'static str> {
            let pin = &mut self.pins[index];
            let result = if energized {
                pin.set_high()
            } else {
                pin.set_low()
            };

            result.map_err(|_| "the relay input could not be driven")
        }
    }

    pub struct ReservoirRelayOutputs<'d>(RelayOutputs<'d>);

    impl<'d> ReservoirRelayOutputs<'d> {
        pub fn new(
            ro_valve: impl OutputPin + 'd,
            booster: impl OutputPin + 'd,
            tap_valve: impl OutputPin + 'd,
            mixer: impl OutputPin + 'd,
            contacts: [Contact; 4],
        ) -> Result<Self, EspError> {
            RelayOutputs::new(ro_valve, booster, tap_valve, mixer, contacts).map(Self)
        }

        pub fn set_coil(
            &mut self,
            channel: ReservoirChannel,
            energized: bool,
        ) -> Result<(), &'static str> {
            self.0.drive(channel.index(), energized)
        }
    }

    impl ReservoirOutputs for ReservoirRelayOutputs<'_> {
        fn set(&mut self, channel: ReservoirChannel, on: bool) -> Result<(), &'static str> {
            self.set_coil(channel, self.0.contacts[channel.index()].coil_for(on))
        }
    }

    impl PumpOutputs for RelayOutputs<'_> {
        fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
            let coil = self.contacts[id.index()].coil_for(on);
            self.set_coil(id, coil)
        }
    }
}
