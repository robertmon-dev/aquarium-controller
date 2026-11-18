#[cfg(target_os = "espidf")]
pub use hw::NvsCalibration;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::domain::calibration::{Calibration, PhCalibration};
    use aquarium_core::ports::outbound::CalibrationPersistence;

    const NAMESPACE: &str = "aquarium";
    const LAYOUT_VERSION: u8 = 1;

    const KEY_VERSION: &str = "cal_ver";
    const KEY_PH_NEUTRAL: &str = "ph_neutral";
    const KEY_PH_ACID: &str = "ph_acid";
    const KEY_PH_CELSIUS: &str = "ph_cal_c";
    const KEY_MAKEUP_CELSIUS: &str = "makeup_c";
    const KEY_TDS_FACTOR: &str = "tds_k";

    pub struct NvsCalibration {
        nvs: EspNvs<NvsDefault>,
    }

    impl NvsCalibration {
        pub fn new(partition: EspDefaultNvsPartition) -> Result<Self, EspError> {
            Ok(Self {
                nvs: EspNvs::new(partition, NAMESPACE, true)?,
            })
        }

        fn read_f32(&self, key: &str) -> Option<f32> {
            self.nvs.get_u32(key).ok().flatten().map(f32::from_bits)
        }

        fn write_f32(&self, key: &str, value: f32) -> Result<(), EspError> {
            self.nvs.set_u32(key, value.to_bits())
        }

        fn write_all(&self, value: &Calibration) -> Result<(), EspError> {
            self.write_f32(KEY_PH_NEUTRAL, value.ph.neutral_volts)?;
            self.write_f32(KEY_PH_ACID, value.ph.acid_volts)?;
            self.write_f32(KEY_PH_CELSIUS, value.ph.calibrated_at_celsius)?;
            self.write_f32(KEY_MAKEUP_CELSIUS, value.makeup_tank_celsius)?;
            self.write_f32(KEY_TDS_FACTOR, value.tds_factor)?;
            self.nvs.set_u8(KEY_VERSION, LAYOUT_VERSION)
        }
    }

    impl CalibrationPersistence for NvsCalibration {
        fn load(&self) -> Option<Calibration> {
            if self.nvs.get_u8(KEY_VERSION).ok().flatten()? != LAYOUT_VERSION {
                return None;
            }

            let stored = Calibration {
                ph: PhCalibration {
                    neutral_volts: self.read_f32(KEY_PH_NEUTRAL)?,
                    acid_volts: self.read_f32(KEY_PH_ACID)?,
                    calibrated_at_celsius: self.read_f32(KEY_PH_CELSIUS)?,
                },
                makeup_tank_celsius: self.read_f32(KEY_MAKEUP_CELSIUS)?,
                tds_factor: self.read_f32(KEY_TDS_FACTOR)?,
            };

            stored.validate().ok().map(|()| stored)
        }

        fn store(&self, value: &Calibration) -> Result<(), &'static str> {
            self.write_all(value)
                .map_err(|_| "the calibration could not be written to NVS")
        }
    }
}
