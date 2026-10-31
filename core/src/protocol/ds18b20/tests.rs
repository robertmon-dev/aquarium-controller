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
