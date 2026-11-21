#[cfg(target_os = "espidf")]
pub use hw::NvsCalibration;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::domain::calibration::Calibration;
    use aquarium_core::ports::outbound::CalibrationPersistence;
    use aquarium_core::protocol::settings_record::{
        decode_calibration, encode_calibration, legacy, legacy_calibration, restore, Restored,
        CALIBRATION_RECORD_KEY,
    };

    use crate::adapters::outbound::nvs_blob::{read_blob, replace_blob};

    const NAMESPACE: &str = "aquarium";
    const MAX_RECORD_BYTES: usize = 512;

    pub struct NvsCalibration {
        nvs: Option<EspNvs<NvsDefault>>,
    }

    impl NvsCalibration {
        pub fn new(partition: Option<EspDefaultNvsPartition>) -> Result<Self, EspError> {
            Ok(Self {
                nvs: partition
                    .map(|partition| EspNvs::new(partition, NAMESPACE, true))
                    .transpose()?,
            })
        }
    }

    fn write(nvs: &EspNvs<NvsDefault>, value: &Calibration) -> Result<(), &'static str> {
        let bytes = encode_calibration(value)?;

        replace_blob(nvs, CALIBRATION_RECORD_KEY, &bytes)
            .map_err(|_| "the calibration could not be written to NVS")
    }

    impl CalibrationPersistence for NvsCalibration {
        fn load(&self) -> Option<Calibration> {
            let nvs = self.nvs.as_ref()?;

            let blob = match read_blob(nvs, CALIBRATION_RECORD_KEY, MAX_RECORD_BYTES) {
                Ok(blob) => blob,
                Err(error) => {
                    tracing::warn!(%error, "calibration record unreadable");
                    return None;
                }
            };

            let legacy_layout = || {
                legacy_calibration(
                    nvs.get_u8(legacy::CALIBRATION_VERSION).ok().flatten(),
                    |key| nvs.get_u32(key).ok().flatten(),
                )
            };

            match restore(blob.as_deref(), decode_calibration, legacy_layout) {
                Restored::Current(value) => Some(value),
                Restored::Migrated(value) => {
                    match write(nvs, &value) {
                        Ok(()) => tracing::info!("calibration migrated to a single NVS record"),
                        Err(reason) => tracing::warn!(reason, "calibration not migrated"),
                    }

                    Some(value)
                }
                Restored::Missing => None,
            }
        }

        fn store(&self, value: &Calibration) -> Result<(), &'static str> {
            let nvs = self
                .nvs
                .as_ref()
                .ok_or("NVS is unavailable, the calibration cannot be saved")?;

            write(nvs, value)
        }
    }
}
