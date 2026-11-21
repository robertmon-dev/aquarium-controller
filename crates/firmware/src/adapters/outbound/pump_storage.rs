#[cfg(target_os = "espidf")]
pub use hw::PumpStorage;

#[cfg(target_os = "espidf")]
mod hw {
    use aquarium_core::ports::outbound::{PumpRecord, PumpStore};

    use crate::adapters::outbound::nvs_pump::NvsPumpStore;

    pub enum PumpStorage {
        Nvs(NvsPumpStore),
        Unavailable,
    }

    impl PumpStore for PumpStorage {
        fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
            match self {
                Self::Nvs(store) => store.load(record),
                Self::Unavailable => None,
            }
        }

        fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
            match self {
                Self::Nvs(store) => store.store(record, bytes),
                Self::Unavailable => Err("NVS is unavailable, the pump record cannot be saved"),
            }
        }

        fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
            match self {
                Self::Nvs(store) => store.erase(record),
                Self::Unavailable => Err("NVS is unavailable, the pump record cannot be erased"),
            }
        }
    }
}
