pub use aquarium_core::protocol::ds18b20::RomAddress;

#[cfg(target_os = "espidf")]
pub use hw::{Ds18b20Adapter, OneWireBus};

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::delay::Ets;
    use esp_idf_hal::gpio::{InputOutput, InputPin, OutputPin, PinDriver, Pull};
    use esp_idf_hal::interrupt;
    use tokio::task::yield_now;
    use tokio::time::{sleep, Duration, Instant};

    use aquarium_core::domain::metrics::WaterTemperature;
    use aquarium_core::ports::outbound::temperature::{
        TemperatureError, TemperatureReader, TemperatureSensorRole,
    };
    use aquarium_core::protocol::ds18b20::{
        convert_all, parse_scratchpad, read_scratchpad, validate_rom, OneWireLine, RomAddress,
    };

    const CMD_SKIP_ROM: u8 = 0xCC;
    const CMD_READ_POWER_SUPPLY: u8 = 0xB4;

    const CONVERSION_TIMEOUT: Duration = Duration::from_millis(800);
    const CONVERSION_POLL_INTERVAL: Duration = Duration::from_millis(10);

    pub struct OneWireBus<'d> {
        pin: PinDriver<'d, InputOutput>,
    }

    impl<'d> OneWireBus<'d> {
        pub fn new(pin: impl InputPin + OutputPin + 'd) -> Result<Self, TemperatureError> {
            let mut driver = PinDriver::input_output_od(pin, Pull::Up).map_err(|_| {
                TemperatureError::BusError("Failed to initialize Open-Drain GPIO driver")
            })?;

            driver
                .set_high()
                .map_err(|_| TemperatureError::BusError("Failed to release 1-Wire line"))?;

            Ok(Self { pin: driver })
        }

        pub fn has_parasite_powered_device(&mut self) -> Result<bool, TemperatureError> {
            self.reset()?;
            self.write_byte(CMD_SKIP_ROM);
            self.write_byte(CMD_READ_POWER_SUPPLY);
            Ok(!self.read_bit())
        }
    }

    impl OneWireLine for OneWireBus<'_> {
        fn reset(&mut self) -> Result<(), TemperatureError> {
            if self.pin.is_low() {
                return Err(TemperatureError::BusError(
                    "1-Wire line stuck LOW before reset (short to GND?)",
                ));
            }

            self.pin
                .set_low()
                .map_err(|_| TemperatureError::BusError("Failed to pull line LOW during reset"))?;
            Ets::delay_us(480);

            let presence = interrupt::free(|| {
                let _ = self.pin.set_high();
                Ets::delay_us(70);
                self.pin.is_low()
            });

            Ets::delay_us(410);

            if self.pin.is_low() {
                return Err(TemperatureError::BusError(
                    "1-Wire line did not return HIGH after presence pulse",
                ));
            }

            if presence {
                Ok(())
            } else {
                Err(TemperatureError::BusError(
                    "No presence pulse detected on 1-Wire bus",
                ))
            }
        }

        fn write_bit(&mut self, bit: bool) {
            if bit {
                interrupt::free(|| {
                    let _ = self.pin.set_low();
                    Ets::delay_us(6);
                    let _ = self.pin.set_high();
                });
                Ets::delay_us(64);
            } else {
                interrupt::free(|| {
                    let _ = self.pin.set_low();
                    Ets::delay_us(60);
                    let _ = self.pin.set_high();
                });
                Ets::delay_us(10);
            }
        }

        fn read_bit(&mut self) -> bool {
            let bit = interrupt::free(|| {
                let _ = self.pin.set_low();
                Ets::delay_us(3);
                let _ = self.pin.set_high();
                Ets::delay_us(10);
                self.pin.is_high()
            });

            Ets::delay_us(55);
            bit
        }
    }

    pub struct Ds18b20Adapter<'d> {
        bus: OneWireBus<'d>,
        sensors: [(TemperatureSensorRole, RomAddress); 2],
    }

    impl<'d> Ds18b20Adapter<'d> {
        pub fn new(
            mut bus: OneWireBus<'d>,
            tank_rom: RomAddress,
            sump_rom: RomAddress,
        ) -> Result<Self, TemperatureError> {
            validate_rom(&tank_rom)?;
            validate_rom(&sump_rom)?;

            if tank_rom == sump_rom {
                return Err(TemperatureError::BusError(
                    "Tank and sump sensors are configured with the same ROM address",
                ));
            }

            if bus.has_parasite_powered_device()? {
                return Err(TemperatureError::BusError(
                    "Parasite-powered DS18B20 detected; sensors must be powered from VDD",
                ));
            }

            Ok(Self {
                bus,
                sensors: [
                    (TemperatureSensorRole::MainTank, tank_rom),
                    (TemperatureSensorRole::Sump, sump_rom),
                ],
            })
        }

        fn rom_for(&self, role: TemperatureSensorRole) -> Result<RomAddress, TemperatureError> {
            self.sensors
                .iter()
                .find(|(r, _)| *r == role)
                .map(|(_, rom)| *rom)
                .ok_or(TemperatureError::SensorNotFound(role))
        }
    }

    impl<'d> TemperatureReader for Ds18b20Adapter<'d> {
        async fn trigger_conversion(&mut self) -> Result<(), TemperatureError> {
            convert_all(&mut self.bus)?;

            let deadline = Instant::now() + CONVERSION_TIMEOUT;
            loop {
                sleep(CONVERSION_POLL_INTERVAL).await;

                if self.bus.read_bit() {
                    return Ok(());
                }
                if Instant::now() >= deadline {
                    return Err(TemperatureError::BusError(
                        "Temperature conversion timed out",
                    ));
                }
            }
        }

        async fn read_temperature(
            &mut self,
            role: TemperatureSensorRole,
        ) -> Result<WaterTemperature, TemperatureError> {
            let rom = self.rom_for(role)?;
            let scratchpad = read_scratchpad(&mut self.bus, &rom)?;
            yield_now().await;

            parse_scratchpad(&scratchpad, role)
        }
    }
}
