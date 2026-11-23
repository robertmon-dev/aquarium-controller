use std::sync::Arc;

use crate::domain::reservoir::{ReservoirChannel, ReservoirTelemetry};

pub trait ReservoirOutputs {
    fn set(&mut self, channel: ReservoirChannel, on: bool) -> Result<(), &'static str>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReservoirRecord {
    Settings,
    Running,
    Last,
    Runtime,
    Fault,
}

impl ReservoirRecord {
    pub const COUNT: usize = 5;

    pub const fn index(self) -> usize {
        match self {
            Self::Settings => 0,
            Self::Running => 1,
            Self::Last => 2,
            Self::Runtime => 3,
            Self::Fault => 4,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Settings => "res_cfg",
            Self::Running => "res_run",
            Self::Last => "res_last",
            Self::Runtime => "res_rt",
            Self::Fault => "res_fault",
        }
    }
}

pub trait ReservoirStore {
    fn load(&self, record: ReservoirRecord) -> Result<Option<Vec<u8>>, &'static str>;
    fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str>;
    fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str>;
}

impl<S: ReservoirStore + ?Sized> ReservoirStore for Arc<S> {
    fn load(&self, record: ReservoirRecord) -> Result<Option<Vec<u8>>, &'static str> {
        self.as_ref().load(record)
    }

    fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.as_ref().store(record, bytes)
    }

    fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str> {
        self.as_ref().erase(record)
    }
}

pub trait ReservoirStatusSource {
    fn reservoir_telemetry(&self) -> ReservoirTelemetry;
}

impl<T: ReservoirStatusSource + ?Sized> ReservoirStatusSource for Arc<T> {
    fn reservoir_telemetry(&self) -> ReservoirTelemetry {
        self.as_ref().reservoir_telemetry()
    }
}

pub trait LeakSource {
    fn leak(&self) -> Option<bool>;
}

impl<T: LeakSource + ?Sized> LeakSource for Arc<T> {
    fn leak(&self) -> Option<bool> {
        self.as_ref().leak()
    }
}

pub trait WaterChangeActivity {
    fn water_change_active(&self) -> bool;
}

impl<T: WaterChangeActivity + ?Sized> WaterChangeActivity for Arc<T> {
    fn water_change_active(&self) -> bool {
        self.as_ref().water_change_active()
    }
}

impl<T: WaterChangeActivity> WaterChangeActivity for Option<T> {
    fn water_change_active(&self) -> bool {
        self.as_ref()
            .is_some_and(WaterChangeActivity::water_change_active)
    }
}
