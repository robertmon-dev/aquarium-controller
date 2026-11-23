use anyhow::Context;
use aquarium_controller::adapters::outbound::ads1115::{Ads1115Bus, AnalogFrontEnd};
use aquarium_controller::adapters::outbound::ds18b20::OneWireBus;
use aquarium_controller::adapters::outbound::relay::{RelayOutputs, ReservoirRelayOutputs};
use aquarium_controller::adapters::outbound::ssr::SsrOutputs;
use aquarium_core::protocol::ads1115::Channel;
use esp_idf_hal::gpio::{
    AnyInputPin, Gpio16, Gpio17, Gpio18, Gpio38, Gpio4, Gpio40, Gpio41, Gpio42, Gpio47, Gpio48,
    Gpio5, Gpio6, Gpio7, Gpio8, Gpio9,
};
use esp_idf_hal::i2c::{I2cConfig, I2cDriver, I2C0};
use esp_idf_hal::modem::Modem;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_hal::task::watchdog::TWDT;
use esp_idf_hal::units::FromValueType;

use crate::SETTINGS;

const I2C_BAUDRATE_KHZ: u32 = 400;

pub struct Board {
    pub pumps: PumpPins,
    pub reservoir: ReservoirPins,
    pub flood: (Gpio38<'static>, Gpio48<'static>),
    pub heaters: HeaterPins,
    pub temperature: TemperaturePin,
    pub levels: [AnyInputPin<'static>; 8],
    pub analog: AnalogBus,
    pub modem: Modem<'static>,
    pub twdt: TWDT<'static>,
}

impl From<Peripherals> for Board {
    fn from(peripherals: Peripherals) -> Self {
        let pins = peripherals.pins;
        Self {
            pumps: PumpPins(pins.gpio5, pins.gpio6, pins.gpio7, pins.gpio16),
            reservoir: ReservoirPins(pins.gpio40, pins.gpio41, pins.gpio42, pins.gpio47),
            flood: (pins.gpio38, pins.gpio48),
            heaters: HeaterPins(pins.gpio17, pins.gpio18),
            temperature: TemperaturePin(pins.gpio4),
            levels: [
                pins.gpio11.degrade_input(),
                pins.gpio12.degrade_input(),
                pins.gpio13.degrade_input(),
                pins.gpio14.degrade_input(),
                pins.gpio15.degrade_input(),
                pins.gpio21.degrade_input(),
                pins.gpio10.degrade_input(),
                pins.gpio39.degrade_input(),
            ],
            analog: AnalogBus(peripherals.i2c0, pins.gpio8, pins.gpio9),
            modem: peripherals.modem,
            twdt: peripherals.twdt,
        }
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

pub struct AnalogBus(I2C0<'static>, Gpio8<'static>, Gpio9<'static>);

impl AnalogBus {
    pub fn open(
        self,
        ph_channel: Option<Channel>,
        tds_channel: Option<Channel>,
    ) -> anyhow::Result<AnalogFrontEnd<'static>> {
        let Self(i2c, sda, scl) = self;
        let analog = &SETTINGS.sensors.analog;
        let config = I2cConfig::new().baudrate(I2C_BAUDRATE_KHZ.kHz().into());

        let driver = I2cDriver::new(i2c, sda, scl, &config)
            .context("Failed to initialize the I2C bus for the ADS1115")?;

        Ok(AnalogFrontEnd::new(
            Ads1115Bus::new(driver, analog.address, analog.gain, analog.data_rate),
            ph_channel,
            tds_channel,
            analog.samples_per_reading,
        ))
    }
}
