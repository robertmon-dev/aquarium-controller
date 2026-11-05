use crate::domain::metrics::WaterTemperature;
use crate::ports::outbound::temperature::{TemperatureError, TemperatureSensorRole};

const DS18B20_FAMILY_CODE: u8 = 0x28;
const CMD_SEARCH_ROM: u8 = 0xF0;
const CMD_MATCH_ROM: u8 = 0x55;
const CMD_SKIP_ROM: u8 = 0xCC;
const CMD_CONVERT_T: u8 = 0x44;
const CMD_READ_SCRATCHPAD: u8 = 0xBE;

pub const SCRATCHPAD_BYTES: usize = 9;
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

pub fn temperature_from_scratchpad(
    scratchpad: &[u8; SCRATCHPAD_BYTES],
) -> Result<WaterTemperature, TemperatureError> {
    if !check_crc(scratchpad) {
        return Err(TemperatureError::BusError("Scratchpad CRC check failed"));
    }

    let raw = i16::from_le_bytes([scratchpad[0], scratchpad[1]]);
    if raw == POWER_ON_RESET_RAW {
        return Err(TemperatureError::PowerOnResetValue);
    }

    let celsius = raw_to_celsius(raw, scratchpad[4]);
    WaterTemperature::new(celsius).map_err(TemperatureError::InvalidReading)
}

pub fn parse_scratchpad(
    scratchpad: &[u8; SCRATCHPAD_BYTES],
    role: TemperatureSensorRole,
) -> Result<WaterTemperature, TemperatureError> {
    temperature_from_scratchpad(scratchpad).map_err(|error| match error {
        TemperatureError::BusError(_) => TemperatureError::CrcMismatch(role),
        other => other,
    })
}

pub trait OneWireLine {
    fn reset(&mut self) -> Result<(), TemperatureError>;

    fn read_bit(&mut self) -> bool;

    fn write_bit(&mut self, bit: bool);

    fn read_byte(&mut self) -> u8 {
        let mut byte = 0u8;
        for index in 0..8 {
            if self.read_bit() {
                byte |= 1 << index;
            }
        }
        byte
    }

    fn write_byte(&mut self, mut byte: u8) {
        for _ in 0..8 {
            self.write_bit(byte & 0x01 != 0);
            byte >>= 1;
        }
    }
}

#[derive(Debug, Default)]
pub struct RomSearch {
    rom: RomAddress,
    last_discrepancy: u8,
    done: bool,
}

impl RomSearch {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next<L: OneWireLine>(
        &mut self,
        line: &mut L,
    ) -> Result<Option<RomAddress>, TemperatureError> {
        if self.done {
            return Ok(None);
        }

        if let Err(error) = line.reset() {
            self.restart();
            return Err(error);
        }

        line.write_byte(CMD_SEARCH_ROM);

        let mut last_zero = 0u8;

        for position in 1..=64u8 {
            let id_bit = line.read_bit();
            let complement = line.read_bit();

            if id_bit && complement {
                self.restart();
                return Ok(None);
            }

            let direction = if id_bit != complement {
                id_bit
            } else {
                let chosen = if position < self.last_discrepancy {
                    self.bit(position)
                } else {
                    position == self.last_discrepancy
                };

                if !chosen {
                    last_zero = position;
                }

                chosen
            };

            self.set_bit(position, direction);
            line.write_bit(direction);
        }

        if let Err(error) = validate_rom(&self.rom) {
            self.restart();
            return Err(error);
        }

        self.last_discrepancy = last_zero;
        self.done = last_zero == 0;

        Ok(Some(self.rom))
    }

    fn bit(&self, position: u8) -> bool {
        let index = usize::from(position - 1);
        self.rom[index / 8] & (1 << (index % 8)) != 0
    }

    fn set_bit(&mut self, position: u8, value: bool) {
        let index = usize::from(position - 1);
        let mask = 1 << (index % 8);

        if value {
            self.rom[index / 8] |= mask;
        } else {
            self.rom[index / 8] &= !mask;
        }
    }

    fn restart(&mut self) {
        self.rom = [0u8; 8];
        self.last_discrepancy = 0;
        self.done = false;
    }
}

pub fn search_roms<L: OneWireLine>(
    line: &mut L,
    out: &mut [RomAddress],
) -> Result<usize, TemperatureError> {
    let mut search = RomSearch::new();
    let mut count = 0;

    while count < out.len() {
        match search.next(line)? {
            Some(rom) => {
                out[count] = rom;
                count += 1;
            }
            None => break,
        }
    }

    Ok(count)
}

pub fn convert_all<L: OneWireLine>(line: &mut L) -> Result<(), TemperatureError> {
    line.reset()?;
    line.write_byte(CMD_SKIP_ROM);
    line.write_byte(CMD_CONVERT_T);

    Ok(())
}

pub fn read_scratchpad<L: OneWireLine>(
    line: &mut L,
    rom: &RomAddress,
) -> Result<[u8; SCRATCHPAD_BYTES], TemperatureError> {
    line.reset()?;
    line.write_byte(CMD_MATCH_ROM);

    for &byte in rom {
        line.write_byte(byte);
    }

    line.write_byte(CMD_READ_SCRATCHPAD);

    let mut scratchpad = [0u8; SCRATCHPAD_BYTES];
    for byte in &mut scratchpad {
        *byte = line.read_byte();
    }

    Ok(scratchpad)
}

#[cfg(test)]
mod tests;
