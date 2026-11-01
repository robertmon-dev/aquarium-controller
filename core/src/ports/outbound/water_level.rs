use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot};

pub trait WaterLevelPins {
    fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT];
}

pub trait WaterLevelSink {
    fn publish(&self, snapshot: WaterLevelSnapshot);
}

pub trait WaterLevelSource {
    fn levels(&self) -> WaterLevelSnapshot;
}
