use std::sync::Arc;

use crate::domain::cron::CivilTime;

pub trait Clock {
    fn epoch_seconds(&self) -> Option<u64>;
    fn uptime_seconds(&self) -> u64;
    fn local_minute_of_day(&self) -> Option<u16>;
    fn local_month(&self) -> Option<u8>;
    fn local_civil_time(&self) -> Option<CivilTime>;
}

impl<C: Clock + ?Sized> Clock for Arc<C> {
    fn epoch_seconds(&self) -> Option<u64> {
        self.as_ref().epoch_seconds()
    }

    fn uptime_seconds(&self) -> u64 {
        self.as_ref().uptime_seconds()
    }

    fn local_minute_of_day(&self) -> Option<u16> {
        self.as_ref().local_minute_of_day()
    }

    fn local_month(&self) -> Option<u8> {
        self.as_ref().local_month()
    }

    fn local_civil_time(&self) -> Option<CivilTime> {
        self.as_ref().local_civil_time()
    }
}
