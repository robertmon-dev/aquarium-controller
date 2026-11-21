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
        convert_all, parse_scratchpad, probe_bus, read_scratchpad, validate_rom, BusRecovery,
        OneWireLine, RomAddress,
    };
    use tracing::{info, warn};

    const CONVERSION_TIMEOUT: Duration = Duration::from_millis(800);
    const CONVERSION_POLL_INTERVAL: Duration = Duration::from_millis(10);
    const RETRY_MIN: Duration = Duration::from_secs(5);
    const RETRY_MAX: Duration = Duration::from_secs(60);

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
        recovery: BusRecovery,
    }

    impl<'d> Ds18b20Adapter<'d> {
        pub fn new(
            bus: OneWireBus<'d>,
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

            let mut adapter = Self {
                bus,
                sensors: [
                    (TemperatureSensorRole::MainTank, tank_rom),
                    (TemperatureSensorRole::Sump, sump_rom),
                ],
                recovery: BusRecovery::new(RETRY_MIN, RETRY_MAX, Instant::now()),
            };

            let _ = adapter.ensure_bus();

            Ok(adapter)
        }

        fn ensure_bus(&mut self) -> Result<(), TemperatureError> {
            if self.recovery.is_ready() {
                return Ok(());
            }

            let now = Instant::now();
            if !self.recovery.attempt_due(now) {
                return Err(TemperatureError::BusError("1-Wire bus unavailable"));
            }

            match probe_bus(&mut self.bus) {
                Ok(()) => {
                    self.recovery.succeeded();
                    info!("DS18B20 bus available");
                    Ok(())
                }
                Err(error) => {
                    let retry_in = self.recovery.failed(now);
                    warn!(%error, retry_in_s = retry_in.as_secs(), "DS18B20 bus unavailable");
                    Err(error)
                }
            }
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
            self.ensure_bus()?;

            if let Err(error) = convert_all(&mut self.bus) {
                warn!(%error, "DS18B20 bus lost");
                self.recovery.lost(Instant::now());
                return Err(error);
            }

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
