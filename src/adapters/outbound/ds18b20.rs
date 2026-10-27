//! DS18B20 driver for bit-bang 1-Wire (ESP32 / ESP-IDF).
//!
//! Hardware specs:
//! - external pull-up  resistor 4,7 kΩ for 3V3
//! - external pull-up for ESP32 in only addition in the circuit.
//! - all sensors are powered via VDD
//! - `Ds18b20Adapter::new`, shall return result of `TemperatureError` in case of direct powering from ESP32.
//!
//!  Protocol layer (CRC, walidacja ROM, scratchpad parsing) does not depend on
//! `esp_idf_hal`, so unit tests runs on host (via `cargo test`).
use crate::domain::metrics::WaterTemperature;
use crate::ports::outbound::temperature::{TemperatureError, TemperatureSensorRole};

const DS18B20_FAMILY_CODE: u8 = 0x28;
const POWER_ON_RESET_RAW: i16 = 0x0550;

pub type RomAddress = [u8; 8];

pub fn crc8(data: &[u8]) -> u8 {
    let mut crc = 0u8;
    for &byte in data {
        let mut curr = byte;
        for _ in 0..8 {
            let mix = (crc ^ curr) & 0x01;
            crc >>= 1;
            if mix != 0 {
                crc ^= 0x8C;
            }
            curr >>= 1;
        }
    }
    crc
}

pub fn check_crc(data: &[u8]) -> bool {
    !data.is_empty() && data.iter().any(|&b| b != 0) && crc8(data) == 0
}

pub fn validate_rom(rom: &RomAddress) -> Result<(), TemperatureError> {
    if !check_crc(rom) {
        return Err(TemperatureError::BusError("ROM CRC check failed"));
    }
    if rom[0] != DS18B20_FAMILY_CODE {
        return Err(TemperatureError::BusError(
            "Device is not a DS18B20 (invalid family code)",
        ));
    }
    Ok(())
}

fn raw_to_celsius(raw: i16, config: u8) -> f32 {
    let mask: i16 = match (config >> 5) & 0b11 {
        0b00 => !0b111, // 9 bit
        0b01 => !0b011, // 10 bit
        0b10 => !0b001, // 11 bit
        _ => !0,        // 12 bit
    };
    f32::from(raw & mask) * 0.0625
}

pub fn parse_scratchpad(
    scratchpad: &[u8; 9],
    role: TemperatureSensorRole,
) -> Result<WaterTemperature, TemperatureError> {
    if !check_crc(scratchpad) {
        return Err(TemperatureError::CrcMismatch(role));
    }

    let raw = i16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    if raw == POWER_ON_RESET_RAW {
        return Err(TemperatureError::PowerOnResetValue);
    }

    let celsius = raw_to_celsius(raw, scratchpad[4]);
    WaterTemperature::new(celsius).map_err(TemperatureError::InvalidReading)
}

#[cfg(target_os = "espidf")]
pub use hw::{Ds18b20Adapter, OneWireBus};

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::delay::Ets;
    use esp_idf_hal::gpio::{InputOutput, InputPin, OutputPin, PinDriver, Pull};
    use esp_idf_hal::interrupt;
    use tokio::task::yield_now;
    use tokio::time::{sleep, Duration, Instant};

    use super::{parse_scratchpad, validate_rom, RomAddress};
    use crate::domain::metrics::WaterTemperature;
    use crate::ports::outbound::temperature::{
        TemperatureError, TemperatureReader, TemperatureSensorRole,
    };

    const CMD_READ_ROM: u8 = 0x33;
    const CMD_MATCH_ROM: u8 = 0x55;
    const CMD_SKIP_ROM: u8 = 0xCC;
    const CMD_CONVERT_T: u8 = 0x44;
    const CMD_READ_SCRATCHPAD: u8 = 0xBE;
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

        pub fn reset(&mut self) -> Result<(), TemperatureError> {
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

        pub fn write_bit(&mut self, bit: bool) {
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

        pub fn read_bit(&mut self) -> bool {
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

        pub fn write_byte(&mut self, mut byte: u8) {
            for _ in 0..8 {
                self.write_bit((byte & 0x01) != 0);
                byte >>= 1;
            }
        }

        pub fn read_byte(&mut self) -> u8 {
            let mut byte = 0u8;
            for i in 0..8 {
                if self.read_bit() {
                    byte |= 1 << i;
                }
            }
            byte
        }

        pub fn read_single_rom(&mut self) -> Result<RomAddress, TemperatureError> {
            self.reset()?;
            self.write_byte(CMD_READ_ROM);

            let mut rom = [0u8; 8];
            for byte in &mut rom {
                *byte = self.read_byte();
            }

            validate_rom(&rom)?;
            Ok(rom)
        }

        pub fn has_parasite_powered_device(&mut self) -> Result<bool, TemperatureError> {
            self.reset()?;
            self.write_byte(CMD_SKIP_ROM);
            self.write_byte(CMD_READ_POWER_SUPPLY);
            Ok(!self.read_bit())
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

        async fn write_bytes(&mut self, bytes: &[u8]) {
            for &byte in bytes {
                self.bus.write_byte(byte);
                yield_now().await;
            }
        }

        async fn read_bytes(&mut self, buf: &mut [u8]) {
            for byte in buf.iter_mut() {
                *byte = self.bus.read_byte();
                yield_now().await;
            }
        }
    }

    impl<'d> TemperatureReader for Ds18b20Adapter<'d> {
        async fn trigger_conversion(&mut self) -> Result<(), TemperatureError> {
            self.bus.reset()?;
            self.bus.write_byte(CMD_SKIP_ROM);
            self.bus.write_byte(CMD_CONVERT_T);

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

            self.bus.reset()?;
            self.bus.write_byte(CMD_MATCH_ROM);
            self.write_bytes(&rom).await;
            self.bus.write_byte(CMD_READ_SCRATCHPAD);

            let mut scratchpad = [0u8; 9];
            self.read_bytes(&mut scratchpad).await;

            parse_scratchpad(&scratchpad, role)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG_12_BIT: u8 = 0x7F;
    const CONFIG_9_BIT: u8 = 0x1F;

    fn scratchpad(lsb: u8, msb: u8, config: u8) -> [u8; 9] {
        let mut sp = [lsb, msb, 0x4B, 0x46, config, 0xFF, 0x0C, 0x10, 0x00];
        sp[8] = crc8(&sp[..8]);
        sp
    }

    fn rom_with_crc(bytes: [u8; 7]) -> RomAddress {
        let mut rom = [0u8; 8];
        rom[..7].copy_from_slice(&bytes);
        rom[7] = crc8(&bytes);
        rom
    }

    fn approx_eq(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn crc8_matches_maxim_reference_vector() {
        assert_eq!(crc8(&[0x02, 0x1C, 0xB8, 0x01, 0x00, 0x00, 0x00]), 0xA2);
    }

    #[test]
    fn check_crc_rejects_all_zeros() {
        assert!(!check_crc(&[0u8; 9]));
    }

    #[test]
    fn check_crc_rejects_all_ones_disconnected_sensor() {
        assert!(!check_crc(&[0xFFu8; 9]));
    }

    #[test]
    fn check_crc_rejects_empty() {
        assert!(!check_crc(&[]));
    }

    #[test]
    fn validate_rom_accepts_ds18b20() {
        let rom = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
        assert!(validate_rom(&rom).is_ok());
    }

    #[test]
    fn validate_rom_rejects_wrong_family() {
        let rom = rom_with_crc([0x10, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
        assert!(matches!(
            validate_rom(&rom),
            Err(TemperatureError::BusError(_))
        ));
    }

    #[test]
    fn validate_rom_rejects_bad_crc() {
        let mut rom = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
        rom[7] ^= 0x01;
        assert!(validate_rom(&rom).is_err());
    }

    #[test]
    fn converts_datasheet_values_12_bit() {
        assert!(approx_eq(raw_to_celsius(0x07D0, CONFIG_12_BIT), 125.0));
        assert!(approx_eq(raw_to_celsius(0x0191, CONFIG_12_BIT), 25.0625));
        assert!(approx_eq(raw_to_celsius(0x0000, CONFIG_12_BIT), 0.0));
        assert!(approx_eq(
            raw_to_celsius(0xFF5Eu16 as i16, CONFIG_12_BIT),
            -10.125
        ));
        assert!(approx_eq(
            raw_to_celsius(0xFC90u16 as i16, CONFIG_12_BIT),
            -55.0
        ));
    }

    #[test]
    fn masks_undefined_bits_at_9_bit_resolution() {
        assert!(approx_eq(raw_to_celsius(0x01CF, CONFIG_9_BIT), 28.5));
    }

    #[test]
    fn rejects_all_zeros_bus_short() {
        assert!(matches!(
            parse_scratchpad(&[0u8; 9], TemperatureSensorRole::MainTank),
            Err(TemperatureError::CrcMismatch(
                TemperatureSensorRole::MainTank
            ))
        ));
    }

    #[test]
    fn rejects_corrupted_scratchpad() {
        let mut sp = scratchpad(0xC8, 0x01, CONFIG_12_BIT);
        sp[0] ^= 0x04;
        assert!(matches!(
            parse_scratchpad(&sp, TemperatureSensorRole::Sump),
            Err(TemperatureError::CrcMismatch(TemperatureSensorRole::Sump))
        ));
    }

    #[test]
    fn detects_power_on_reset_85c() {
        let sp = scratchpad(0x50, 0x05, CONFIG_12_BIT);
        assert!(matches!(
            parse_scratchpad(&sp, TemperatureSensorRole::Sump),
            Err(TemperatureError::PowerOnResetValue)
        ));
    }

    #[test]
    fn parses_valid_discus_temperature_28_5c() {
        let sp = scratchpad(0xC8, 0x01, CONFIG_12_BIT);
        let temp = parse_scratchpad(&sp, TemperatureSensorRole::MainTank).unwrap();
        assert!(approx_eq(temp.as_celsius(), 28.5));
    }
}
