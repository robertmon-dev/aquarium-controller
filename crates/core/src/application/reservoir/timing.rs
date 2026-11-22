use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReservoirTiming {
    pub tick: Duration,
    pub level_stale: Duration,
    pub tds_stale: Duration,
    pub runtime_flush: Duration,
}

impl ReservoirTiming {
    pub const DEFAULT: Self = Self {
        tick: Duration::from_secs(1),
        level_stale: Duration::from_secs(1),
        tds_stale: Duration::from_secs(5),
        runtime_flush: Duration::from_secs(600),
    };
}
