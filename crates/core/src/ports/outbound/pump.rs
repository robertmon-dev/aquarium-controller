use std::sync::Arc;

use crate::domain::pump::{PumpId, PumpsTelemetry};

pub trait PumpOutputs {
    fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PumpRecord {
    Settings,
    Overrides,
    Checkpoint,
    Runtime,
    LastOperations,
}

impl PumpRecord {
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        match self {
            Self::Settings => 0,
            Self::Overrides => 1,
            Self::Checkpoint => 2,
            Self::Runtime => 3,
            Self::LastOperations => 4,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Settings => "pump_cfg",
            Self::Overrides => "pump_ovr",
            Self::Checkpoint => "pump_op",
            Self::Runtime => "pump_rt",
            Self::LastOperations => "pump_last",
        }
    }
}

pub trait PumpStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>>;
    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str>;
    fn erase(&self, record: PumpRecord) -> Result<(), &'static str>;
}

impl<S: PumpStore + ?Sized> PumpStore for Arc<S> {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.as_ref().load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.as_ref().store(record, bytes)
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        self.as_ref().erase(record)
    }
}

pub trait PumpStatusSource {
    fn pump_telemetry(&self) -> PumpsTelemetry;
}

impl<T: PumpStatusSource + ?Sized> PumpStatusSource for Arc<T> {
    fn pump_telemetry(&self) -> PumpsTelemetry {
        self.as_ref().pump_telemetry()
    }
}
