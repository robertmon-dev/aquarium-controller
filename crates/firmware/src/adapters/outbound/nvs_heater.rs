#[cfg(target_os = "espidf")]
pub use hw::NvsHeaterSettings;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::domain::heater::{HeaterSettings, ZoneSettings};
    use aquarium_core::ports::outbound::HeaterPersistence;

    const NAMESPACE: &str = "aquarium";
    const LAYOUT_VERSION: u8 = 1;

    const KEY_VERSION: &str = "heat_ver";
    const KEY_TANK_TARGET: &str = "heat_tank_t";
    const KEY_TANK_HYSTERESIS: &str = "heat_tank_h";
    const KEY_SUMP_TARGET: &str = "heat_sump_t";
    const KEY_SUMP_HYSTERESIS: &str = "heat_sump_h";
    const KEY_CUTOFF: &str = "heat_cutoff";

    pub struct NvsHeaterSettings {
        nvs: EspNvs<NvsDefault>,
    }

    impl NvsHeaterSettings {
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

        fn write_all(&self, value: &HeaterSettings) -> Result<(), EspError> {
            self.write_f32(KEY_TANK_TARGET, value.tank.target_celsius)?;
            self.write_f32(KEY_TANK_HYSTERESIS, value.tank.hysteresis_celsius)?;
            self.write_f32(KEY_SUMP_TARGET, value.sump.target_celsius)?;
            self.write_f32(KEY_SUMP_HYSTERESIS, value.sump.hysteresis_celsius)?;
            self.write_f32(KEY_CUTOFF, value.cutoff_celsius)?;
            self.nvs.set_u8(KEY_VERSION, LAYOUT_VERSION)
        }
    }

    impl HeaterPersistence for NvsHeaterSettings {
        fn load(&self) -> Option<HeaterSettings> {
            if self.nvs.get_u8(KEY_VERSION).ok().flatten()? != LAYOUT_VERSION {
                return None;
            }

            let stored = HeaterSettings {
                tank: ZoneSettings {
                    target_celsius: self.read_f32(KEY_TANK_TARGET)?,
                    hysteresis_celsius: self.read_f32(KEY_TANK_HYSTERESIS)?,
                },
                sump: ZoneSettings {
                    target_celsius: self.read_f32(KEY_SUMP_TARGET)?,
                    hysteresis_celsius: self.read_f32(KEY_SUMP_HYSTERESIS)?,
                },
                cutoff_celsius: self.read_f32(KEY_CUTOFF)?,
            };

            stored.validate().ok().map(|()| stored)
        }

        fn store(&self, value: &HeaterSettings) -> Result<(), &'static str> {
            self.write_all(value)
                .map_err(|_| "the heater settings could not be written to NVS")
        }
    }
}
