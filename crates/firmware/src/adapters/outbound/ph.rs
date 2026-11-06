pub use aquarium_core::protocol::ph::PhCalibration;

#[cfg(target_os = "espidf")]
pub use hw::PhProbe;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_hal::adc::attenuation;
    use esp_idf_hal::adc::oneshot::config::{AdcChannelConfig, Calibration};
    use esp_idf_hal::adc::oneshot::{AdcChannelDriver, AdcDriver};
    use esp_idf_hal::adc::{Adc, AdcChannel};
    use esp_idf_hal::gpio::ADCPin;
    use esp_idf_hal::sys::EspError;
    use tokio::task::yield_now;

    use aquarium_core::ports::outbound::ph::PhReader;
    use aquarium_core::protocol::ph::PhError;

    type Channel<'d, C> = AdcChannelDriver<'d, C, AdcDriver<'d, <C as AdcChannel>::AdcUnit>>;

    pub struct PhProbe<'d, C: AdcChannel> {
        channel: Channel<'d, C>,
        samples: u16,
    }

    impl<'d, C: AdcChannel> PhProbe<'d, C> {
        pub fn new<ADC>(
            adc: ADC,
            pin: impl ADCPin<AdcChannel = C> + 'd,
            samples: u16,
        ) -> Result<Self, EspError>
        where
            ADC: Adc<AdcUnit = C::AdcUnit> + 'd,
        {
            let config = AdcChannelConfig {
                attenuation: attenuation::DB_12,
                calibration: Calibration::Curve,
                ..Default::default()
            };

            Ok(Self {
                channel: AdcChannelDriver::new(AdcDriver::new(adc)?, pin, &config)?,
                samples: samples.max(1),
            })
        }
    }

    impl<C: AdcChannel> PhReader for PhProbe<'_, C> {
        async fn read_volts(&mut self) -> Result<f32, PhError> {
            let mut total = 0u32;

            for _ in 0..self.samples {
                let millivolts = self
                    .channel
                    .read()
                    .map_err(|_| PhError::Unavailable("ADC read failed"))?;

                total += u32::from(millivolts);
                yield_now().await;
            }

            Ok(total as f32 / f32::from(self.samples) / 1000.0)
        }
    }
}
