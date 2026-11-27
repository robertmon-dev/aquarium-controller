#[cfg(target_os = "espidf")]
pub use hw::NvsLightingPalette;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::domain::lighting::LightingPalette;
    use aquarium_core::ports::outbound::LightingPersistence;
    use aquarium_core::protocol::settings_record::{
        decode_lighting, encode_lighting, restore, Restored, LIGHTING_RECORD_KEY,
    };

    use crate::adapters::outbound::nvs_blob::{read_blob, replace_blob};

    const NAMESPACE: &str = "aquarium";
    const MAX_RECORD_BYTES: usize = 512;

    pub struct NvsLightingPalette {
        nvs: Option<EspNvs<NvsDefault>>,
    }

    impl NvsLightingPalette {
        pub fn new(partition: Option<EspDefaultNvsPartition>) -> Result<Self, EspError> {
            Ok(Self {
                nvs: partition
                    .map(|partition| EspNvs::new(partition, NAMESPACE, true))
                    .transpose()?,
            })
        }
    }

    impl LightingPersistence for NvsLightingPalette {
        fn load(&self) -> Option<LightingPalette> {
            let nvs = self.nvs.as_ref()?;

            let blob = match read_blob(nvs, LIGHTING_RECORD_KEY, MAX_RECORD_BYTES) {
                Ok(blob) => blob,
                Err(error) => {
                    tracing::warn!(%error, "lighting palette record unreadable");
                    return None;
                }
            };

            match restore(blob.as_deref(), decode_lighting, || None) {
                Restored::Current(palette) | Restored::Migrated(palette) => Some(palette),
                Restored::Missing => None,
            }
        }

        fn store(&self, value: &LightingPalette) -> Result<(), &'static str> {
            let nvs = self
                .nvs
                .as_ref()
                .ok_or("NVS is unavailable, the lighting palette cannot be saved")?;
            let bytes = encode_lighting(value)?;

            replace_blob(nvs, LIGHTING_RECORD_KEY, &bytes)
                .map_err(|_| "the lighting palette could not be written to NVS")
        }
    }
}
