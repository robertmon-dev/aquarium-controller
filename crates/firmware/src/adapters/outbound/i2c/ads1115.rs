#[cfg(target_os = "espidf")]
pub use hw::{Ads1115Bus, AnalogFrontEnd};

#[cfg(target_os = "espidf")]
mod hw {
    use std::time::Duration;

    use esp_idf_hal::sys::EspError;
    use tokio::time::sleep;

    use aquarium_core::ports::outbound::{PhReader, TdsReader};
    use aquarium_core::protocol::ads1115::{
        config_register, conversion_time, volts_from_raw, Channel, DataRate, Gain, REG_CONFIG,
        REG_CONVERSION,
    };
    use aquarium_core::protocol::ph::PhError;
    use aquarium_core::protocol::tds::TdsError;

    use super::super::shared::SharedI2c;

    const CONFIG_OS_IDLE: u16 = 0x8000;

    const READY_POLL_STEPS: u32 = 16;
    const READY_POLL_MIN_INTERVAL: Duration = Duration::from_micros(100);

    pub struct Ads1115Bus<'d> {
        i2c: SharedI2c<'d>,
        address: u8,
        gain: Gain,
        rate: DataRate,
    }

    impl<'d> Ads1115Bus<'d> {
        pub fn new(i2c: SharedI2c<'d>, address: u8, gain: Gain, rate: DataRate) -> Self {
            Self {
                i2c,
                address,
                gain,
                rate,
            }
        }

        fn start_conversion(&mut self, channel: Channel) -> Result<(), EspError> {
            let [high, low] = config_register(channel, self.gain, self.rate);

            self.i2c.write(self.address, &[REG_CONFIG, high, low])
        }

        fn conversion_ready(&mut self) -> Result<bool, EspError> {
            let mut buffer = [0_u8; 2];

            self.i2c
                .write_read(self.address, &[REG_CONFIG], &mut buffer)?;

            Ok(u16::from_be_bytes(buffer) & CONFIG_OS_IDLE != 0)
        }

        async fn wait_for_conversion(&mut self) -> Result<(), &'static str> {
            let nominal = conversion_time(self.rate);
            let interval = (nominal / READY_POLL_STEPS).max(READY_POLL_MIN_INTERVAL);

            sleep(nominal).await;

            for _ in 0..=READY_POLL_STEPS {
                let ready = self
                    .conversion_ready()
                    .map_err(|_| "ADS1115 did not return its status")?;

                if ready {
                    return Ok(());
                }

                sleep(interval).await;
            }

            Err("ADS1115 conversion did not complete in time")
        }

        fn read_raw(&mut self) -> Result<i16, EspError> {
            let mut buffer = [0_u8; 2];

            self.i2c
                .write_read(self.address, &[REG_CONVERSION], &mut buffer)?;

            Ok(i16::from_be_bytes(buffer))
        }

        async fn read_mean_raw(
            &mut self,
            channel: Channel,
            samples: u16,
        ) -> Result<i16, &'static str> {
            let samples = samples.max(1);
            let mut total = 0_i32;

            for _ in 0..samples {
                self.start_conversion(channel)
                    .map_err(|_| "ADS1115 did not accept the config write")?;

                self.wait_for_conversion().await?;

                let raw = self
                    .read_raw()
                    .map_err(|_| "ADS1115 did not return a conversion")?;

                total += i32::from(raw);
            }

            Ok((total / i32::from(samples)) as i16)
        }

        async fn read_volts(
            &mut self,
            channel: Channel,
            samples: u16,
        ) -> Result<f32, &'static str> {
            let raw = self.read_mean_raw(channel, samples).await?;

            Ok(volts_from_raw(raw, self.gain))
        }

        pub async fn read_raw_and_volts(
            &mut self,
            channel: Channel,
            samples: u16,
        ) -> Result<(i16, f32), &'static str> {
            let raw = self.read_mean_raw(channel, samples).await?;

            Ok((raw, volts_from_raw(raw, self.gain)))
        }
    }

    pub struct AnalogFrontEnd<'d> {
        bus: Ads1115Bus<'d>,
        ph_channel: Option<Channel>,
        tds_channel: Option<Channel>,
        samples: u16,
    }

    impl<'d> AnalogFrontEnd<'d> {
        pub fn new(
            bus: Ads1115Bus<'d>,
            ph_channel: Option<Channel>,
            tds_channel: Option<Channel>,
            samples: u16,
        ) -> Self {
            Self {
                bus,
                ph_channel,
                tds_channel,
                samples,
            }
        }

        pub async fn probe(&mut self, channel: Channel) -> Result<(i16, f32), &'static str> {
            self.bus.read_raw_and_volts(channel, self.samples).await
        }
    }

    impl PhReader for AnalogFrontEnd<'_> {
        async fn read_volts(&mut self) -> Result<f32, PhError> {
            let channel = self
                .ph_channel
                .ok_or(PhError::Unavailable("no ADS1115 channel assigned to pH"))?;

            self.bus
                .read_volts(channel, self.samples)
                .await
                .map_err(PhError::Unavailable)
        }
    }

    impl TdsReader for AnalogFrontEnd<'_> {
        async fn read_volts(&mut self) -> Result<f32, TdsError> {
            let channel = self
                .tds_channel
                .ok_or(TdsError::Unavailable("no ADS1115 channel assigned to TDS"))?;

            self.bus
                .read_volts(channel, self.samples)
                .await
                .map_err(TdsError::Unavailable)
        }
    }
}
