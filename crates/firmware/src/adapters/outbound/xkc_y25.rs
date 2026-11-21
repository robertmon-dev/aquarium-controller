pub use aquarium_core::protocol::xkc_y25::{FilterConfig, Polarity};

#[cfg(target_os = "espidf")]
pub use hw::Xkc25Adapter;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{AnyInputPin, Input, PinDriver, Pull};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::domain::water::WaterLevelRole;
    use aquarium_core::ports::outbound::water_level::WaterLevelPins;

    type LevelPin<'d> = PinDriver<'d, Input>;

    pub struct Xkc25Adapter<'d> {
        pins: Vec<LevelPin<'d>>,
    }

    impl<'d> Xkc25Adapter<'d> {
        pub fn new(pins: [AnyInputPin<'d>; WaterLevelRole::COUNT]) -> Result<Self, EspError> {
            let pins = pins
                .into_iter()
                .map(|pin| PinDriver::input(pin, Pull::Up))
                .collect::<Result<_, _>>()?;

            Ok(Self { pins })
        }
    }

    impl WaterLevelPins for Xkc25Adapter<'_> {
        fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT] {
            WaterLevelRole::ALL.map(|role| (role, self.pins[role.index()].is_high()))
        }
    }
}
