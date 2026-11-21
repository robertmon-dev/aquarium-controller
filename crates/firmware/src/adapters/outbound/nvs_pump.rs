#[cfg(target_os = "espidf")]
pub use hw::NvsPumpStore;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::ports::outbound::{PumpRecord, PumpStore};

    const NAMESPACE: &str = "aquarium";
    const MAX_BLOB_BYTES: usize = 2048;

    pub struct NvsPumpStore {
        nvs: EspNvs<NvsDefault>,
    }

    impl NvsPumpStore {
        pub fn new(partition: EspDefaultNvsPartition) -> Result<Self, EspError> {
            Ok(Self {
                nvs: EspNvs::new(partition, NAMESPACE, true)?,
            })
        }
    }

    impl PumpStore for NvsPumpStore {
        fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
            let length = match self.nvs.blob_len(record.key()) {
                Ok(Some(length)) => length.min(MAX_BLOB_BYTES),
                Ok(None) => return None,
                Err(error) => {
                    tracing::warn!(%error, key = record.key(), "pump record length unreadable");
                    return None;
                }
            };

            let mut buffer = vec![0_u8; length];

            match self.nvs.get_blob(record.key(), &mut buffer) {
                Ok(found) => found.map(<[u8]>::to_vec),
                Err(error) => {
                    tracing::warn!(%error, key = record.key(), "pump record unreadable");
                    None
                }
            }
        }

        fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
            if bytes.len() > MAX_BLOB_BYTES {
                return Err("the pump record exceeds the NVS blob limit");
            }

            self.nvs
                .set_blob(record.key(), bytes)
                .map_err(|_| "the pump record could not be written to NVS")
        }

        fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
            self.nvs
                .remove(record.key())
                .map(|_| ())
                .map_err(|_| "the pump record could not be erased from NVS")
        }
    }
}
