use std::collections::VecDeque;

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

fn rom_bit(rom: &RomAddress, index: usize) -> bool {
    rom[index / 8] & (1 << (index % 8)) != 0
}

struct FakeBus {
    devices: Vec<RomAddress>,
    path: Vec<bool>,
    searching: bool,
    pending_complement: Option<bool>,
    command_bits: Vec<bool>,
    commands: Vec<u8>,
    responses: VecDeque<bool>,
}

impl FakeBus {
    fn new(devices: &[RomAddress]) -> Self {
        Self {
            devices: devices.to_vec(),
            path: Vec::new(),
            searching: false,
            pending_complement: None,
            command_bits: Vec::new(),
            commands: Vec::new(),
            responses: VecDeque::new(),
        }
    }

    fn queue_response(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            for index in 0..8 {
                self.responses.push_back(byte & (1 << index) != 0);
            }
        }
    }

    fn candidates(&self) -> Vec<RomAddress> {
        self.devices
            .iter()
            .copied()
            .filter(|rom| {
                self.path
                    .iter()
                    .enumerate()
                    .all(|(index, &bit)| rom_bit(rom, index) == bit)
            })
            .collect()
    }
}

impl OneWireLine for FakeBus {
    fn reset(&mut self) -> Result<(), TemperatureError> {
        self.path.clear();
        self.searching = false;
        self.pending_complement = None;
        self.command_bits.clear();

        if self.devices.is_empty() {
            return Err(TemperatureError::BusError(
                "No presence pulse detected on 1-Wire bus",
            ));
        }

        Ok(())
    }

    fn read_bit(&mut self) -> bool {
        if let Some(complement) = self.pending_complement.take() {
            return complement;
        }

        if !self.searching {
            return self.responses.pop_front().unwrap_or(true);
        }

        let candidates = self.candidates();
        let index = self.path.len();
        let id_bit = candidates.iter().all(|rom| rom_bit(rom, index));
        let complement = candidates.iter().all(|rom| !rom_bit(rom, index));

        self.pending_complement = Some(complement);
        id_bit
    }

    fn write_bit(&mut self, bit: bool) {
        if self.searching {
            self.pending_complement = None;
            self.path.push(bit);
            return;
        }

        self.command_bits.push(bit);
        if self.command_bits.len() < 8 {
            return;
        }

        let byte = self
            .command_bits
            .iter()
            .enumerate()
            .fold(0u8, |acc, (index, &bit)| acc | (u8::from(bit) << index));

        self.command_bits.clear();
        self.commands.push(byte);

        if byte == 0xF0 {
            self.searching = true;
            self.path.clear();
        }
    }
}

#[test]
fn rom_search_finds_a_single_device() {
    let rom = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    let mut bus = FakeBus::new(&[rom]);
    let mut search = RomSearch::new();

    assert_eq!(search.next(&mut bus).unwrap(), Some(rom));
    assert_eq!(search.next(&mut bus).unwrap(), None);
}

fn drain_search(devices: &[RomAddress]) -> Vec<RomAddress> {
    let mut bus = FakeBus::new(devices);
    let mut search = RomSearch::new();
    let mut found = Vec::new();

    while let Some(rom) = search.next(&mut bus).unwrap() {
        found.push(rom);
        assert!(
            found.len() <= devices.len() + 1,
            "search does not terminate"
        );
    }

    found.sort_unstable();
    found
}

#[test]
fn rom_search_enumerates_two_devices_sharing_a_bus() {
    let tank = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    let sump = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x02]);

    let mut expected = vec![tank, sump];
    expected.sort_unstable();

    assert_eq!(drain_search(&[tank, sump]), expected);
}

#[test]
fn rom_search_enumerates_devices_differing_in_the_first_byte() {
    let early = rom_with_crc([0x28, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00]);
    let late = rom_with_crc([0x28, 0xFE, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]);

    let mut expected = vec![early, late];
    expected.sort_unstable();

    assert_eq!(drain_search(&[early, late]), expected);
}

#[test]
fn rom_search_enumerates_four_devices_without_duplicates() {
    let devices = [
        rom_with_crc([0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x01]),
        rom_with_crc([0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x02]),
        rom_with_crc([0x28, 0x91, 0x22, 0x33, 0x44, 0x55, 0x01]),
        rom_with_crc([0x28, 0xF1, 0xA2, 0xB3, 0xC4, 0xD5, 0xE6]),
    ];

    let found = drain_search(&devices);
    let mut expected = devices.to_vec();
    expected.sort_unstable();

    assert_eq!(found, expected);
}

#[test]
fn rom_search_reports_a_bus_with_no_presence_pulse() {
    let mut bus = FakeBus::new(&[]);
    let mut search = RomSearch::new();

    assert!(matches!(
        search.next(&mut bus),
        Err(TemperatureError::BusError(_))
    ));
}

#[test]
fn rom_search_rejects_a_device_with_a_broken_rom_crc() {
    let mut corrupted = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    corrupted[7] ^= 0xFF;

    let mut bus = FakeBus::new(&[corrupted]);
    let mut search = RomSearch::new();

    assert!(matches!(
        search.next(&mut bus),
        Err(TemperatureError::BusError("ROM CRC check failed"))
    ));
}

#[test]
fn rom_search_rejects_a_device_from_another_family() {
    let other = rom_with_crc([0x10, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);

    let mut bus = FakeBus::new(&[other]);
    let mut search = RomSearch::new();

    assert!(matches!(
        search.next(&mut bus),
        Err(TemperatureError::BusError(_))
    ));
}

#[test]
fn rom_search_issues_the_search_command_lsb_first() {
    let rom = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    let mut bus = FakeBus::new(&[rom]);
    let mut search = RomSearch::new();

    search.next(&mut bus).unwrap();

    assert_eq!(bus.commands, vec![0xF0]);
}

#[test]
fn search_roms_reports_how_many_devices_it_stored() {
    let devices = [
        rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]),
        rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x02]),
    ];

    let mut bus = FakeBus::new(&devices);
    let mut found = [[0u8; 8]; 4];

    let count = search_roms(&mut bus, &mut found).unwrap();

    let mut stored = found[..count].to_vec();
    stored.sort_unstable();
    let mut expected = devices.to_vec();
    expected.sort_unstable();

    assert_eq!(count, 2);
    assert_eq!(stored, expected);
}

#[test]
fn search_roms_stops_once_the_buffer_is_full() {
    let devices = [
        rom_with_crc([0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x01]),
        rom_with_crc([0x28, 0x11, 0x22, 0x33, 0x44, 0x55, 0x02]),
        rom_with_crc([0x28, 0x91, 0x22, 0x33, 0x44, 0x55, 0x01]),
    ];

    let mut bus = FakeBus::new(&devices);
    let mut found = [[0u8; 8]; 2];

    assert_eq!(search_roms(&mut bus, &mut found).unwrap(), 2);
}

#[test]
fn convert_all_issues_skip_rom_then_convert() {
    let rom = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    let mut bus = FakeBus::new(&[rom]);

    convert_all(&mut bus).unwrap();

    assert_eq!(bus.commands, vec![0xCC, 0x44]);
}

#[test]
fn read_scratchpad_addresses_one_sensor_and_returns_its_bytes() {
    let rom = rom_with_crc([0x28, 0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]);
    let expected_scratchpad = scratchpad(0xC8, 0x01, CONFIG_12_BIT);

    let mut bus = FakeBus::new(&[rom]);
    bus.queue_response(&expected_scratchpad);

    assert_eq!(
        read_scratchpad(&mut bus, &rom).unwrap(),
        expected_scratchpad
    );

    let mut expected_commands = vec![0x55];
    expected_commands.extend_from_slice(&rom);
    expected_commands.push(0xBE);

    assert_eq!(bus.commands, expected_commands);
}

#[test]
fn temperature_from_scratchpad_parses_without_a_sensor_role() {
    let sp = scratchpad(0xC8, 0x01, CONFIG_12_BIT);
    let temperature = temperature_from_scratchpad(&sp).unwrap();

    assert!(approx_eq(temperature.as_celsius(), 28.5));
}

#[test]
fn temperature_from_scratchpad_reports_a_bad_crc_without_a_role() {
    let mut sp = scratchpad(0xC8, 0x01, CONFIG_12_BIT);
    sp[0] ^= 0x04;

    assert!(matches!(
        temperature_from_scratchpad(&sp),
        Err(TemperatureError::BusError(_))
    ));
}

#[test]
fn temperature_from_scratchpad_still_detects_power_on_reset() {
    let sp = scratchpad(0x50, 0x05, CONFIG_12_BIT);

    assert!(matches!(
        temperature_from_scratchpad(&sp),
        Err(TemperatureError::PowerOnResetValue)
    ));
}
