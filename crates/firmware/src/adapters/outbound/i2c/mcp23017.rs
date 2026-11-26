#[cfg(target_os = "espidf")]
pub use hw::Mcp23017;

#[cfg(target_os = "espidf")]
mod hw {
    use std::sync::{Arc, Mutex};

    use esp_idf_hal::sys::EspError;

    use super::super::shared::SharedI2c;

    const REG_IODIRA: u8 = 0x00;
    const REG_GPPUA: u8 = 0x0C;
    const REG_GPIOA: u8 = 0x12;
    const REG_OLATA: u8 = 0x14;

    struct Mcp23017State<'d> {
        i2c: SharedI2c<'d>,
        address: u8,
        outputs: u16,
    }

    impl<'d> Mcp23017State<'d> {
        fn write_outputs(&mut self, next: u16) -> Result<(), EspError> {
            let [port_a, port_b] = next.to_le_bytes();
            self.i2c.write(self.address, &[REG_OLATA, port_a, port_b])?;
            self.outputs = next;
            Ok(())
        }
    }

    #[derive(Clone)]
    pub struct Mcp23017<'d> {
        state: Arc<Mutex<Mcp23017State<'d>>>,
    }

    impl<'d> Mcp23017<'d> {
        pub fn new(
            i2c: SharedI2c<'d>,
            address: u8,
            input_mask: u16,
            pullup_mask: u16,
        ) -> Result<Self, EspError> {
            let [dir_a, dir_b] = input_mask.to_le_bytes();
            let [pu_a, pu_b] = pullup_mask.to_le_bytes();

            i2c.write(address, &[REG_OLATA, 0x00, 0x00])?;
            i2c.write(address, &[REG_GPPUA, pu_a, pu_b])?;
            i2c.write(address, &[REG_IODIRA, dir_a, dir_b])?;

            Ok(Self {
                state: Arc::new(Mutex::new(Mcp23017State {
                    i2c,
                    address,
                    outputs: 0x0000,
                })),
            })
        }

        pub fn read_all(&self) -> Result<u16, EspError> {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let mut buf = [0_u8; 2];
            state
                .i2c
                .write_read(state.address, &[REG_GPIOA], &mut buf)?;
            Ok(u16::from_le_bytes(buf))
        }

        pub fn set_pin(&self, pin: u8, high: bool) -> Result<(), &'static str> {
            if pin >= 16 {
                return Err("MCP23017 pin out of range");
            }

            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let mask = 1_u16 << pin;
            let next = if high {
                state.outputs | mask
            } else {
                state.outputs & !mask
            };

            state
                .write_outputs(next)
                .map_err(|_| "MCP23017 did not accept the output write")
        }
    }
}
