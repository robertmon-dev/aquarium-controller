#[cfg(target_os = "espidf")]
pub use hw::NvsPumpStore;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::ports::outbound::{PumpRecord, PumpStore};

    use crate::adapters::outbound::nvs_blob::{read_blob, replace_blob};

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
        fn load_flood_alarm(&self) -> Result<Option<Vec<u8>>, &'static str> {
            read_blob(&self.nvs, PumpRecord::Flood.key(), MAX_BLOB_BYTES)
                .map_err(|_| "the saved flood alarm could not be read")
        }

        fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
            read_blob(&self.nvs, record.key(), MAX_BLOB_BYTES).unwrap_or_else(|error| {
                tracing::warn!(%error, key = record.key(), "pump record unreadable");
                None
            })
        }

        fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
            if bytes.len() > MAX_BLOB_BYTES {
                return Err("the pump record exceeds the NVS blob limit");
            }

            replace_blob(&self.nvs, record.key(), bytes)
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
