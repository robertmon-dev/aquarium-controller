pub use aquarium_core::protocol::xkc_y25::{FilterConfig, Polarity};

#[cfg(target_os = "espidf")]
pub use hw::Xkc25Adapter;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::gpio::{Input, InputPin, PinDriver, Pull};
    use esp_idf_hal::sys::EspError;

    use aquarium_core::domain::water::WaterLevelRole;
    use aquarium_core::ports::outbound::water_level::WaterLevelPins;

    type LevelPin<'d> = PinDriver<'d, Input>;

    pub struct Xkc25Adapter<'d> {
        pins: [(WaterLevelRole, LevelPin<'d>); WaterLevelRole::COUNT],
    }

    impl<'d> Xkc25Adapter<'d> {
        pub fn new(
            mechanical: impl InputPin + 'd,
            biological: impl InputPin + 'd,
            pump: impl InputPin + 'd,
            tank: impl InputPin + 'd,
            reserve: impl InputPin + 'd,
        ) -> Result<Self, EspError> {
            Ok(Self {
                pins: [
                    (
                        WaterLevelRole::SumpMechanical,
                        PinDriver::input(mechanical, Pull::Up)?,
                    ),
                    (
                        WaterLevelRole::SumpBiological,
                        PinDriver::input(biological, Pull::Up)?,
                    ),
                    (WaterLevelRole::SumpPump, PinDriver::input(pump, Pull::Up)?),
                    (WaterLevelRole::MainTank, PinDriver::input(tank, Pull::Up)?),
                    (
                        WaterLevelRole::SumpReserve,
                        PinDriver::input(reserve, Pull::Up)?,
                    ),
                ],
            })
        }
    }

    impl WaterLevelPins for Xkc25Adapter<'_> {
        fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT] {
            let mut raw = WaterLevelRole::ALL.map(|role| (role, false));

            for (slot, (role, pin)) in raw.iter_mut().zip(self.pins.iter()) {
                *slot = (*role, pin.is_high());
            }

            raw
        }
    }
}
