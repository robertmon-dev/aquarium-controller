use std::time::Duration;

use super::*;

#[test]
fn single_shot_config_for_a0_at_the_four_volt_range() {
    assert_eq!(
        config_register(Channel::A0, Gain::Fsr4_096, DataRate::Sps128),
        [0xC3, 0x83]
    );
}

#[test]
fn single_shot_config_for_a1_at_the_four_volt_range() {
    assert_eq!(
        config_register(Channel::A1, Gain::Fsr4_096, DataRate::Sps128),
        [0xD3, 0x83]
    );
}

#[test]
fn the_mux_field_is_the_only_difference_between_channels() {
    let a0 = u16::from_be_bytes(config_register(
        Channel::A0,
        Gain::Fsr4_096,
        DataRate::Sps128,
    ));
    let a3 = u16::from_be_bytes(config_register(
        Channel::A3,
        Gain::Fsr4_096,
        DataRate::Sps128,
    ));

    assert_eq!(a0 ^ a3, 0b011 << 12);
}

#[test]
fn every_config_starts_a_conversion_in_single_shot_with_the_comparator_off() {
    for channel in [Channel::A0, Channel::A1, Channel::A2, Channel::A3] {
        let value = u16::from_be_bytes(config_register(channel, Gain::Fsr2_048, DataRate::Sps8));

        assert_eq!(value & (1 << 15), 1 << 15, "OS must request a conversion");
        assert_eq!(value & (1 << 8), 1 << 8, "MODE must be single-shot");
        assert_eq!(value & 0b11, 0b11, "the comparator queue must be disabled");
    }
}

#[test]
fn each_gain_writes_its_own_pga_field() {
    let bits = |gain| {
        (u16::from_be_bytes(config_register(Channel::A0, gain, DataRate::Sps128)) >> 9) & 0b111
    };

    assert_eq!(bits(Gain::Fsr6_144), 0b000);
    assert_eq!(bits(Gain::Fsr4_096), 0b001);
    assert_eq!(bits(Gain::Fsr2_048), 0b010);
    assert_eq!(bits(Gain::Fsr1_024), 0b011);
    assert_eq!(bits(Gain::Fsr0_512), 0b100);
    assert_eq!(bits(Gain::Fsr0_256), 0b101);
}

#[test]
fn each_data_rate_writes_its_own_dr_field() {
    let bits = |rate| {
        (u16::from_be_bytes(config_register(Channel::A0, Gain::Fsr4_096, rate)) >> 5) & 0b111
    };

    assert_eq!(bits(DataRate::Sps8), 0b000);
    assert_eq!(bits(DataRate::Sps16), 0b001);
    assert_eq!(bits(DataRate::Sps128), 0b100);
    assert_eq!(bits(DataRate::Sps860), 0b111);
}

#[test]
fn full_scale_raw_maps_to_the_gain_range() {
    assert!((volts_from_raw(32_767, Gain::Fsr4_096) - 4.096).abs() < 0.001);
}

#[test]
fn a_negative_raw_reading_maps_to_a_negative_voltage() {
    assert!((volts_from_raw(-8_000, Gain::Fsr4_096) + 1.0).abs() < 0.001);
}

#[test]
fn zero_raw_is_zero_volts() {
    assert_eq!(volts_from_raw(0, Gain::Fsr4_096), 0.0);
}

#[test]
fn one_lsb_at_the_four_volt_range_is_one_hundred_and_twenty_five_microvolts() {
    assert!((volts_from_raw(1, Gain::Fsr4_096) - 0.000_125).abs() < 1e-9);
}

#[test]
fn a_narrower_range_resolves_a_smaller_voltage_per_step() {
    assert!(volts_from_raw(1, Gain::Fsr0_256) < volts_from_raw(1, Gain::Fsr4_096));
}

#[test]
fn conversion_time_carries_a_margin_over_the_nominal_period() {
    assert_eq!(
        conversion_time(DataRate::Sps128),
        Duration::from_micros(9_374)
    );
    assert_eq!(
        conversion_time(DataRate::Sps8),
        Duration::from_micros(150_000)
    );
}

#[test]
fn a_faster_data_rate_converts_sooner() {
    assert!(conversion_time(DataRate::Sps860) < conversion_time(DataRate::Sps128));
}
