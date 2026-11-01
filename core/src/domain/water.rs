#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterLevelState {
    Wet,
    Dry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterLevelRole {
    SumpMechanical,
    SumpBiological,
    SumpPump,
    MainTank,
}

impl WaterLevelRole {
    pub const COUNT: usize = 4;
}
