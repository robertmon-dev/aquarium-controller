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

#[cfg(test)]
mod tests;
