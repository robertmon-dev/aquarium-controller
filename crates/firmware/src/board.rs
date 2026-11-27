use anyhow::Context;
use aquarium_controller::adapters::outbound::ds18b20::OneWireBus;
use aquarium_controller::adapters::outbound::i2c::ads1115::{Ads1115Bus, AnalogFrontEnd};
use aquarium_controller::adapters::outbound::i2c::mcp23017::Mcp23017;
use aquarium_controller::adapters::outbound::i2c::shared::SharedI2c;
use aquarium_controller::adapters::outbound::lighting::RgbwOutputs;
use aquarium_controller::adapters::outbound::pwm::{self, LedcBank};
use aquarium_controller::adapters::outbound::relay::{RelayOutputs, ReservoirRelayOutputs};
use aquarium_controller::adapters::outbound::ssr::SsrOutputs;
use aquarium_controller::adapters::outbound::sump_light::SumpLightOutput;
use aquarium_core::domain::water::WaterLevelRole;
use aquarium_core::protocol::ads1115::Channel;
use esp_idf_hal::gpio::{
    Gpio10, Gpio11, Gpio12, Gpio13, Gpio14, Gpio15, Gpio16, Gpio17, Gpio18, Gpio21, Gpio38, Gpio4,
    Gpio40, Gpio41, Gpio42, Gpio47, Gpio48, Gpio5, Gpio6, Gpio7, Gpio8, Gpio9, Output, PinDriver,
};
use esp_idf_hal::i2c::{I2cConfig, I2cDriver, I2C0};
use esp_idf_hal::ledc::{CHANNEL0, CHANNEL1, CHANNEL2, CHANNEL3, TIMER0};
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::TWDT;
use esp_idf_hal::units::FromValueType;

use crate::SETTINGS;

const I2C_BAUDRATE_KHZ: u32 = 400;
const MCP23017_ADDRESS: u8 = 0x20;
const EXPANDER_INPUT_MASK: u16 = 0x7F7F;
const EXPANDER_PULLUP_MASK: u16 = EXPANDER_INPUT_MASK;

pub const LEVEL_SENSOR_PINS: [u8; WaterLevelRole::COUNT] = [0, 1, 2, 3, 4, 5, 6, 8];

const _: () = {
    let mut i = 0;
    while i < LEVEL_SENSOR_PINS.len() {
        assert!(EXPANDER_INPUT_MASK & (1 << LEVEL_SENSOR_PINS[i]) != 0);
        i += 1;
    }
};

pub struct Board {
    pub i2c: I2cBus,
    pub pumps: PumpPins,
    pub reservoir: ReservoirPins,
    pub flood: (Gpio38<'static>, Gpio48<'static>),
    pub heaters: HeaterPins,
    pub rgbw: RgbwPins,
    pub sump_light: SumpLightPin,
    pub reserved: ReservedOutputPins,
    pub temperature: TemperaturePin,
    pub modem: Modem<'static>,
    pub twdt: TWDT<'static>,
}

impl From<Peripherals> for Board {
    fn from(peripherals: Peripherals) -> Self {
        let pins = peripherals.pins;
        Self {
            i2c: I2cBus(peripherals.i2c0, pins.gpio8, pins.gpio9),
            pumps: PumpPins(pins.gpio5, pins.gpio6, pins.gpio7, pins.gpio16),
            reservoir: ReservoirPins(pins.gpio40, pins.gpio41, pins.gpio42, pins.gpio47),
            flood: (pins.gpio38, pins.gpio48),
            heaters: HeaterPins(pins.gpio17, pins.gpio18),
            rgbw: RgbwPins {
                timer: peripherals.ledc.timer0,
                red: (peripherals.ledc.channel0, pins.gpio10),
                green: (peripherals.ledc.channel1, pins.gpio11),
                blue: (peripherals.ledc.channel2, pins.gpio12),
                white: (peripherals.ledc.channel3, pins.gpio13),
            },
            sump_light: SumpLightPin(pins.gpio14),
            reserved: ReservedOutputPins(pins.gpio15, pins.gpio21),
            temperature: TemperaturePin(pins.gpio4),
            modem: peripherals.modem,
            twdt: peripherals.twdt,
        }
    }
}

pub struct RgbwPins {
    timer: TIMER0<'static>,
    red: (CHANNEL0<'static>, Gpio10<'static>),
    green: (CHANNEL1<'static>, Gpio11<'static>),
    blue: (CHANNEL2<'static>, Gpio12<'static>),
    white: (CHANNEL3<'static>, Gpio13<'static>),
}

impl RgbwPins {
    pub fn open(self, freq_hz: u32) -> anyhow::Result<RgbwOutputs<'static>> {
        let timer: &'static _ = Box::leak(Box::new(
            pwm::timer(self.timer, freq_hz).context("Failed to initialize the RGBW PWM timer")?,
        ));

        let bank = LedcBank::new([
            pwm::off_channel(self.red.0, timer, self.red.1)?,
            pwm::off_channel(self.green.0, timer, self.green.1)?,
            pwm::off_channel(self.blue.0, timer, self.blue.1)?,
            pwm::off_channel(self.white.0, timer, self.white.1)?,
        ]);

        Ok(RgbwOutputs::new(bank))
    }
}

pub struct SumpLightPin(Gpio14<'static>);

impl SumpLightPin {
    pub fn open(self) -> anyhow::Result<SumpLightOutput<'static>> {
        SumpLightOutput::new(self.0).context("Failed to initialize the sump light output")
    }
}

pub struct ReservedOutputPins(Gpio15<'static>, Gpio21<'static>);

impl ReservedOutputPins {
    pub fn hold_low(self) -> anyhow::Result<[PinDriver<'static, Output>; 2]> {
        let mut fans = PinDriver::output(self.0)?;
        fans.set_low()?;
        let mut spare = PinDriver::output(self.1)?;
        spare.set_low()?;
        Ok([fans, spare])
    }
}

pub struct PumpPins(
    Gpio5<'static>,
    Gpio6<'static>,
    Gpio7<'static>,
    Gpio16<'static>,
);

impl PumpPins {
    pub fn open(self) -> anyhow::Result<RelayOutputs<'static>> {
        RelayOutputs::new(self.0, self.1, self.2, self.3, SETTINGS.pumps.contacts)
            .context("Failed to initialize the pump relay outputs")
    }
}

pub struct ReservoirPins(
    Gpio40<'static>,
    Gpio41<'static>,
    Gpio42<'static>,
    Gpio47<'static>,
);

impl ReservoirPins {
    pub fn open(self) -> anyhow::Result<ReservoirRelayOutputs<'static>> {
        ReservoirRelayOutputs::new(self.0, self.1, self.2, self.3, SETTINGS.reservoir.contacts)
            .context("Failed to initialize the barrel relay outputs")
    }
}

pub struct HeaterPins(Gpio17<'static>, Gpio18<'static>);

impl HeaterPins {
    pub fn open(self) -> anyhow::Result<SsrOutputs<'static>> {
        SsrOutputs::new(self.0, self.1).context("Failed to initialize the heater SSR outputs")
    }
}

pub struct TemperaturePin(Gpio4<'static>);

impl TemperaturePin {
    pub fn open(self) -> anyhow::Result<OneWireBus<'static>> {
        OneWireBus::new(self.0).context("Failed to initialize 1-Wire bus")
    }
}

pub struct I2cBus(I2C0<'static>, Gpio8<'static>, Gpio9<'static>);

impl I2cBus {
    pub fn open(self) -> anyhow::Result<SharedI2c<'static>> {
        let Self(i2c, sda, scl) = self;
        let config = I2cConfig::new().baudrate(I2C_BAUDRATE_KHZ.kHz().into());

        let driver =
            I2cDriver::new(i2c, sda, scl, &config).context("Failed to initialize the I2C bus")?;

        Ok(SharedI2c::new(driver))
    }
}

pub fn analog_front_end(
    bus: SharedI2c<'static>,
    ph_channel: Option<Channel>,
    tds_channel: Option<Channel>,
) -> AnalogFrontEnd<'static> {
    let analog = &SETTINGS.sensors.analog;

    AnalogFrontEnd::new(
        Ads1115Bus::new(bus, analog.address, analog.gain, analog.data_rate),
        ph_channel,
        tds_channel,
        analog.samples_per_reading,
    )
}

pub fn level_expander(bus: SharedI2c<'static>) -> anyhow::Result<Mcp23017<'static>> {
    Mcp23017::new(
        bus,
        MCP23017_ADDRESS,
        EXPANDER_INPUT_MASK,
        EXPANDER_PULLUP_MASK,
    )
    .context("Failed to initialize the MCP23017 expander")
}
