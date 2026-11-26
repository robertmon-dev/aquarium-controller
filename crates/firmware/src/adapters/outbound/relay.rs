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
pub use hw::{RelayOutput, RelayOutputs, ReservoirRelayOutputs, SolenoidRelayOutput};

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{Output, OutputPin, PinDriver};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::domain::pump::PumpId;
    use aquarium_core::domain::reservoir::ReservoirChannel;
    use aquarium_core::ports::outbound::{PumpOutputs, ReservoirOutputs};

    use super::Contact;

    fn released<'d>(pin: impl OutputPin + 'd) -> Result<PinDriver<'d, Output>, EspError> {
        let mut driver = PinDriver::output(pin)?;
        driver.set_low()?;
        Ok(driver)
    }

    struct RelayBank<'d, const N: usize> {
        pins: [PinDriver<'d, Output>; N],
        contacts: [Contact; N],
    }

    impl<'d, const N: usize> RelayBank<'d, N> {
        fn new(pins: [PinDriver<'d, Output>; N], contacts: [Contact; N]) -> Self {
            Self { pins, contacts }
        }

        fn set_coil(&mut self, index: usize, energized: bool) -> Result<(), &'static str> {
            let result = if energized {
                self.pins[index].set_high()
            } else {
                self.pins[index].set_low()
            };

            result.map_err(|_| "the relay input could not be driven")
        }

        fn set(&mut self, index: usize, on: bool) -> Result<(), &'static str> {
            self.set_coil(index, self.contacts[index].coil_for(on))
        }
    }

    pub struct RelayOutput<'d>(RelayBank<'d, 1>);

    impl<'d> RelayOutput<'d> {
        pub fn new(pin: impl OutputPin + 'd, contact: Contact) -> Result<Self, EspError> {
            Ok(Self(RelayBank::new([released(pin)?], [contact])))
        }

        pub fn set_coil(&mut self, energized: bool) -> Result<(), &'static str> {
            self.0.set_coil(0, energized)
        }

        pub fn set(&mut self, on: bool) -> Result<(), &'static str> {
            self.0.set(0, on)
        }
    }

    pub struct RelayOutputs<'d>(RelayBank<'d, 4>);

    impl<'d> RelayOutputs<'d> {
        pub fn new(
            return_pump: impl OutputPin + 'd,
            uv: impl OutputPin + 'd,
            makeup: impl OutputPin + 'd,
            drain: impl OutputPin + 'd,
            contacts: [Contact; 4],
        ) -> Result<Self, EspError> {
            Ok(Self(RelayBank::new(
                [
                    released(return_pump)?,
                    released(uv)?,
                    released(makeup)?,
                    released(drain)?,
                ],
                contacts,
            )))
        }

        pub fn set_coil(&mut self, id: PumpId, energized: bool) -> Result<(), &'static str> {
            self.0.set_coil(id.index(), energized)
        }
    }

    impl PumpOutputs for RelayOutputs<'_> {
        fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
            self.0.set(id.index(), on)
        }
    }

    pub struct ReservoirRelayOutputs<'d>(RelayBank<'d, 4>);

    impl<'d> ReservoirRelayOutputs<'d> {
        pub fn new(
            ro_valve: impl OutputPin + 'd,
            booster: impl OutputPin + 'd,
            tap_valve: impl OutputPin + 'd,
            mixer: impl OutputPin + 'd,
            contacts: [Contact; 4],
        ) -> Result<Self, EspError> {
            Ok(Self(RelayBank::new(
                [
                    released(ro_valve)?,
                    released(booster)?,
                    released(tap_valve)?,
                    released(mixer)?,
                ],
                contacts,
            )))
        }

        pub fn set_coil(
            &mut self,
            channel: ReservoirChannel,
            energized: bool,
        ) -> Result<(), &'static str> {
            self.0.set_coil(channel.index(), energized)
        }
    }

    impl ReservoirOutputs for ReservoirRelayOutputs<'_> {
        fn set(&mut self, channel: ReservoirChannel, on: bool) -> Result<(), &'static str> {
            self.0.set(channel.index(), on)
        }
    }

    pub struct SolenoidRelayOutput<'d>(RelayOutput<'d>);

    impl<'d> SolenoidRelayOutput<'d> {
        pub fn new(
            solenoid_valve: impl OutputPin + 'd,
            contact: Contact,
        ) -> Result<Self, EspError> {
            RelayOutput::new(solenoid_valve, contact).map(Self)
        }

        pub fn set_coil(&mut self, energized: bool) -> Result<(), &'static str> {
            self.0.set_coil(energized)
        }

        pub fn set(&mut self, on: bool) -> Result<(), &'static str> {
            self.0.set(on)
        }
    }
}
