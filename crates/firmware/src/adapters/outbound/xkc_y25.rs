pub use aquarium_core::protocol::xkc_y25::{FilterConfig, Polarity};

#[cfg(target_os = "espidf")]
pub use hw::Xkc25Adapter;

#[cfg(target_os = "espidf")]
mod hw {
    use aquarium_core::domain::water::WaterLevelRole;
    use aquarium_core::ports::outbound::water_level::WaterLevelPins;

    use super::super::i2c::mcp23017::Mcp23017;

    pub struct Xkc25Adapter<'d> {
        expander: Mcp23017<'d>,
        pins: [u8; WaterLevelRole::COUNT],
    }

    impl<'d> Xkc25Adapter<'d> {
        pub fn new(expander: Mcp23017<'d>, pins: [u8; WaterLevelRole::COUNT]) -> Self {
            Self { expander, pins }
        }
    }

    impl WaterLevelPins for Xkc25Adapter<'_> {
        fn read_raw(
            &mut self,
        ) -> Result<[(WaterLevelRole, bool); WaterLevelRole::COUNT], &'static str> {
            let bits = self
                .expander
                .read_all()
                .map_err(|_| "MCP23017 did not return the input levels")?;

            Ok(WaterLevelRole::ALL.map(|role| {
                let pin = self.pins[role.index()];
                (role, bits & (1_u16 << pin) != 0)
            }))
        }
    }
}
