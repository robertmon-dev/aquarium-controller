#[cfg(target_os = "espidf")]
pub use hw::NvsHeaterSettings;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::domain::heater::HeaterSettings;
    use aquarium_core::ports::outbound::HeaterPersistence;
    use aquarium_core::protocol::settings_record::{
        decode_heater, encode_heater, legacy, legacy_heater, restore, Restored, HEATER_RECORD_KEY,
    };

    use crate::adapters::outbound::nvs_blob::{read_blob, replace_blob};

    const NAMESPACE: &str = "aquarium";
    const MAX_RECORD_BYTES: usize = 512;

    pub struct NvsHeaterSettings {
        nvs: Option<EspNvs<NvsDefault>>,
    }

    impl NvsHeaterSettings {
        pub fn new(partition: Option<EspDefaultNvsPartition>) -> Result<Self, EspError> {
            Ok(Self {
                nvs: partition
                    .map(|partition| EspNvs::new(partition, NAMESPACE, true))
                    .transpose()?,
            })
        }
    }

    fn write(nvs: &EspNvs<NvsDefault>, value: &HeaterSettings) -> Result<(), &'static str> {
        let bytes = encode_heater(value)?;

        replace_blob(nvs, HEATER_RECORD_KEY, &bytes)
            .map_err(|_| "the heater settings could not be written to NVS")
    }

    impl HeaterPersistence for NvsHeaterSettings {
        fn load(&self) -> Option<HeaterSettings> {
            let nvs = self.nvs.as_ref()?;

            let blob = match read_blob(nvs, HEATER_RECORD_KEY, MAX_RECORD_BYTES) {
                Ok(blob) => blob,
                Err(error) => {
                    tracing::warn!(%error, "heater settings record unreadable");
                    return None;
                }
            };

            let legacy_layout = || {
                legacy_heater(nvs.get_u8(legacy::HEATER_VERSION).ok().flatten(), |key| {
                    nvs.get_u32(key).ok().flatten()
                })
            };

            match restore(blob.as_deref(), decode_heater, legacy_layout) {
                Restored::Current(settings) => Some(settings),
                Restored::Migrated(settings) => {
                    match write(nvs, &settings) {
                        Ok(()) => tracing::info!("heater settings migrated to a single NVS record"),
                        Err(reason) => tracing::warn!(reason, "heater settings not migrated"),
                    }

                    Some(settings)
                }
                Restored::Missing => None,
            }
        }

        fn store(&self, value: &HeaterSettings) -> Result<(), &'static str> {
            let nvs = self
                .nvs
                .as_ref()
                .ok_or("NVS is unavailable, the heater settings cannot be saved")?;

            write(nvs, value)
        }
    }
}
