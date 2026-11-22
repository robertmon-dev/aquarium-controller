#[cfg(target_os = "espidf")]
pub use hw::NvsReservoirStore;

#[cfg(target_os = "espidf")]
mod hw {
    use esp_idf_svc::nvs::{EspDefaultNvsPartition, EspNvs, NvsDefault};
    use esp_idf_svc::sys::EspError;

    use aquarium_core::ports::outbound::{ReservoirRecord, ReservoirStore};

    use crate::adapters::outbound::nvs_blob::{read_blob, replace_blob};

    const NAMESPACE: &str = "reservoir";
    const MAX_BLOB_BYTES: usize = 1024;

    pub struct NvsReservoirStore {
        nvs: EspNvs<NvsDefault>,
    }

    impl NvsReservoirStore {
        pub fn new(partition: EspDefaultNvsPartition) -> Result<Self, EspError> {
            Ok(Self {
                nvs: EspNvs::new(partition, NAMESPACE, true)?,
            })
        }
    }

    impl ReservoirStore for NvsReservoirStore {
        fn load(&self, record: ReservoirRecord) -> Result<Option<Vec<u8>>, &'static str> {
            read_blob(&self.nvs, record.key(), MAX_BLOB_BYTES).map_err(|error| {
                tracing::warn!(%error, key = record.key(), "barrel record unreadable");
                "the barrel record could not be read from NVS"
            })
        }

        fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str> {
            if bytes.len() > MAX_BLOB_BYTES {
                return Err("the barrel record exceeds the NVS blob limit");
            }
            replace_blob(&self.nvs, record.key(), bytes)
                .map_err(|_| "the barrel record could not be written to NVS")
        }

        fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str> {
            self.nvs
                .remove(record.key())
                .map(|_| ())
                .map_err(|_| "the barrel record could not be erased from NVS")
        }
    }
}
