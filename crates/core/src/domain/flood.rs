use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FloodProbes {
    pub reservoir: bool,
    pub sump: bool,
}

impl FloodProbes {
    pub const fn wet(self) -> bool {
        self.reservoir || self.sump
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub struct FloodStatus {
    pub probes: Option<FloodProbes>,
    pub latched: bool,
}
