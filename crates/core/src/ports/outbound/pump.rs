use std::sync::Arc;

use crate::domain::flood::FloodStatus;
use crate::domain::pump::{OperationRecord, PumpId, PumpsTelemetry};

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
    ReturnHold,
    Flood,
}

impl PumpRecord {
    pub const COUNT: usize = 7;

    pub const fn index(self) -> usize {
        match self {
            Self::Settings => 0,
            Self::Overrides => 1,
            Self::Checkpoint => 2,
            Self::Runtime => 3,
            Self::LastOperations => 4,
            Self::ReturnHold => 5,
            Self::Flood => 6,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Settings => "pump_cfg",
            Self::Overrides => "pump_ovr",
            Self::Checkpoint => "pump_op",
            Self::Runtime => "pump_rt",
            Self::LastOperations => "pump_last",
            Self::ReturnHold => "pump_hold",
            Self::Flood => "flood_alarm",
        }
    }
}

pub trait PumpStore {
    fn load_flood_alarm(&self) -> Result<Option<Vec<u8>>, &'static str> {
        Ok(self.load(PumpRecord::Flood))
    }
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>>;
    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str>;
    fn erase(&self, record: PumpRecord) -> Result<(), &'static str>;
}

impl<S: PumpStore + ?Sized> PumpStore for Arc<S> {
    fn load_flood_alarm(&self) -> Result<Option<Vec<u8>>, &'static str> {
        self.as_ref().load_flood_alarm()
    }

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
    fn flood_status(&self) -> FloodStatus {
        FloodStatus::default()
    }
    fn pump_telemetry(&self) -> PumpsTelemetry;
    fn last_top_off(&self) -> Option<OperationRecord>;
}

impl<T: PumpStatusSource + ?Sized> PumpStatusSource for Arc<T> {
    fn flood_status(&self) -> FloodStatus {
        self.as_ref().flood_status()
    }

    fn pump_telemetry(&self) -> PumpsTelemetry {
        self.as_ref().pump_telemetry()
    }

    fn last_top_off(&self) -> Option<OperationRecord> {
        self.as_ref().last_top_off()
    }
}
