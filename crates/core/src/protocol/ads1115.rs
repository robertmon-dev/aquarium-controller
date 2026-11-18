use std::time::Duration;

pub const DEFAULT_ADDRESS: u8 = 0x48;
pub const REG_CONVERSION: u8 = 0x00;
pub const REG_CONFIG: u8 = 0x01;

const OS_START_CONVERSION: u16 = 1 << 15;
const MODE_SINGLE_SHOT: u16 = 1 << 8;
const COMP_QUE_DISABLED: u16 = 0b11;
const CONVERSION_MARGIN_PERCENT: u32 = 20;
const FULL_SCALE_COUNTS: f32 = 32_768.0;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Ads1115Error {
    #[error("ADS1115 I2C transaction failed: {0}")]
    Bus(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    A0,
    A1,
    A2,
    A3,
}

impl Channel {
    const fn mux_bits(self) -> u16 {
        match self {
            Self::A0 => 0b100,
            Self::A1 => 0b101,
            Self::A2 => 0b110,
            Self::A3 => 0b111,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gain {
    Fsr6_144,
    Fsr4_096,
    Fsr2_048,
    Fsr1_024,
    Fsr0_512,
    Fsr0_256,
}

impl Gain {
    const fn pga_bits(self) -> u16 {
        match self {
            Self::Fsr6_144 => 0b000,
            Self::Fsr4_096 => 0b001,
            Self::Fsr2_048 => 0b010,
            Self::Fsr1_024 => 0b011,
            Self::Fsr0_512 => 0b100,
            Self::Fsr0_256 => 0b101,
        }
    }

    pub const fn full_scale_volts(self) -> f32 {
        match self {
            Self::Fsr6_144 => 6.144,
            Self::Fsr4_096 => 4.096,
            Self::Fsr2_048 => 2.048,
            Self::Fsr1_024 => 1.024,
            Self::Fsr0_512 => 0.512,
            Self::Fsr0_256 => 0.256,
        }
    }

    pub fn lsb_volts(self) -> f32 {
        self.full_scale_volts() / FULL_SCALE_COUNTS
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataRate {
    Sps8,
    Sps16,
    Sps32,
    Sps64,
    Sps128,
    Sps250,
    Sps475,
    Sps860,
}

impl DataRate {
    const fn dr_bits(self) -> u16 {
        match self {
            Self::Sps8 => 0b000,
            Self::Sps16 => 0b001,
            Self::Sps32 => 0b010,
            Self::Sps64 => 0b011,
            Self::Sps128 => 0b100,
            Self::Sps250 => 0b101,
            Self::Sps475 => 0b110,
            Self::Sps860 => 0b111,
        }
    }

    pub const fn samples_per_second(self) -> u32 {
        match self {
            Self::Sps8 => 8,
            Self::Sps16 => 16,
            Self::Sps32 => 32,
            Self::Sps64 => 64,
            Self::Sps128 => 128,
            Self::Sps250 => 250,
            Self::Sps475 => 475,
            Self::Sps860 => 860,
        }
    }
}

pub fn config_register(channel: Channel, gain: Gain, rate: DataRate) -> [u8; 2] {
    let value = OS_START_CONVERSION
        | (channel.mux_bits() << 12)
        | (gain.pga_bits() << 9)
        | MODE_SINGLE_SHOT
        | (rate.dr_bits() << 5)
        | COMP_QUE_DISABLED;

    value.to_be_bytes()
}

pub fn volts_from_raw(raw: i16, gain: Gain) -> f32 {
    f32::from(raw) * gain.lsb_volts()
}

pub fn conversion_time(rate: DataRate) -> Duration {
    let nominal_us = 1_000_000 / rate.samples_per_second();
    let margin_us = nominal_us * CONVERSION_MARGIN_PERCENT / 100;

    Duration::from_micros(u64::from(nominal_us + margin_us))
}

#[cfg(test)]
mod tests;
