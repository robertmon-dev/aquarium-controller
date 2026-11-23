use aquarium_core::domain::flood::FloodProbes;
use esp_idf_hal::gpio::{Input, InputPin, PinDriver, Pull};
use esp_idf_hal::sys::EspError;

pub struct FloodInputs<'d> {
    reservoir: PinDriver<'d, Input>,
    sump: PinDriver<'d, Input>,
}

impl<'d> FloodInputs<'d> {
    pub fn new(reservoir: impl InputPin + 'd, sump: impl InputPin + 'd) -> Result<Self, EspError> {
        Ok(Self {
            reservoir: PinDriver::input(reservoir, Pull::Up)?,
            sump: PinDriver::input(sump, Pull::Up)?,
        })
    }

    pub fn read(&self) -> FloodProbes {
        FloodProbes {
            reservoir: self.reservoir.is_high(),
            sump: self.sump.is_high(),
        }
    }
}
