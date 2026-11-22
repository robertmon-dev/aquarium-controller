#[cfg(target_os = "espidf")]
pub use hw::ReservoirStorage;

#[cfg(target_os = "espidf")]
mod hw {
    use aquarium_core::ports::outbound::{ReservoirRecord, ReservoirStore};

    use crate::adapters::outbound::nvs_reservoir::NvsReservoirStore;

    pub enum ReservoirStorage {
        Nvs(NvsReservoirStore),
        Unavailable,
    }

    impl ReservoirStore for ReservoirStorage {
        fn load(&self, record: ReservoirRecord) -> Result<Option<Vec<u8>>, &'static str> {
            match self {
                Self::Nvs(store) => store.load(record),
                Self::Unavailable => Ok(None),
            }
        }

        fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str> {
            match self {
                Self::Nvs(store) => store.store(record, bytes),
                Self::Unavailable => Err("NVS is unavailable, the barrel record cannot be saved"),
            }
        }

        fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str> {
            match self {
                Self::Nvs(store) => store.erase(record),
                Self::Unavailable => Err("NVS is unavailable, the barrel record cannot be erased"),
            }
        }
    }
}
