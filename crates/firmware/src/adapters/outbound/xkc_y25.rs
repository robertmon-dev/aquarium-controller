pub use aquarium_core::protocol::xkc_y25::{FilterConfig, Polarity};

#[cfg(target_os = "espidf")]
pub use hw::Xkc25Adapter;

#[cfg(target_os = "espidf")]
mod hw {
    use tracing::warn;

    use aquarium_core::domain::water::WaterLevelRole;
    use aquarium_core::ports::outbound::water_level::WaterLevelPins;

    use super::super::i2c::mcp23017::Mcp23017;

    pub struct Xkc25Adapter<'d> {
        expander: Mcp23017<'d>,
        pins: [u8; WaterLevelRole::COUNT],
        last_state: u16,
    }

    impl<'d> Xkc25Adapter<'d> {
        pub fn new(expander: Mcp23017<'d>, pins: [u8; WaterLevelRole::COUNT]) -> Self {
            Self {
                expander,
                pins,
                last_state: 0,
            }
        }
    }

    impl WaterLevelPins for Xkc25Adapter<'_> {
        fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT] {
            match self.expander.read_all() {
                Ok(bits) => self.last_state = bits,
                Err(err) => warn!(?err, "Failed to read XKC-Y25 levels from MCP23017"),
            }

            WaterLevelRole::ALL.map(|role| {
                let pin = self.pins[role.index()];
                (role, (self.last_state & (1_u16 << pin)) != 0)
            })
        }
    }
}
