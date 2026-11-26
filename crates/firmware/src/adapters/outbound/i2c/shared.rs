#[cfg(target_os = "espidf")]
pub use hw::SharedI2c;

#[cfg(target_os = "espidf")]
mod hw {
    use std::sync::{Arc, Mutex};

    use esp_idf_hal::delay::TickType;
    use esp_idf_hal::i2c::I2cDriver;
    use esp_idf_hal::sys::{EspError, TickType_t};

    const TRANSACTION_TIMEOUT_MS: u64 = 100;

    #[derive(Clone)]
    pub struct SharedI2c<'d> {
        driver: Arc<Mutex<I2cDriver<'d>>>,
    }

    impl<'d> SharedI2c<'d> {
        pub fn new(driver: I2cDriver<'d>) -> Self {
            Self {
                driver: Arc::new(Mutex::new(driver)),
            }
        }

        const fn timeout() -> TickType_t {
            TickType::new_millis(TRANSACTION_TIMEOUT_MS).ticks()
        }

        pub fn write(&self, address: u8, bytes: &[u8]) -> Result<(), EspError> {
            let mut bus = self.driver.lock().unwrap_or_else(|e| e.into_inner());
            bus.write(address, bytes, Self::timeout())
        }

        pub fn write_read(
            &self,
            address: u8,
            write: &[u8],
            read: &mut [u8],
        ) -> Result<(), EspError> {
            let mut bus = self.driver.lock().unwrap_or_else(|e| e.into_inner());
            bus.write_read(address, write, read, Self::timeout())
        }
    }
}
