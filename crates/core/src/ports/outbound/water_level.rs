use tokio::time::Instant;

use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelSample {
    pub levels: WaterLevelSnapshot,
    pub measured_at: Instant,
}

pub trait WaterLevelPins {
    fn read_raw(&mut self)
        -> Result<[(WaterLevelRole, bool); WaterLevelRole::COUNT], &'static str>;
}

pub trait WaterLevelSink {
    fn publish(&self, snapshot: WaterLevelSnapshot);
}

pub trait WaterLevelSource {
    fn levels(&self) -> Option<LevelSample>;
}
