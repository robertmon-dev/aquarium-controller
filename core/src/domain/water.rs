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

    pub const ALL: [Self; Self::COUNT] = [
        Self::SumpMechanical,
        Self::SumpBiological,
        Self::SumpPump,
        Self::MainTank,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::SumpMechanical => 0,
            Self::SumpBiological => 1,
            Self::SumpPump => 2,
            Self::MainTank => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WaterLevelReading {
    pub role: WaterLevelRole,
    pub state: WaterLevelState,
    pub faulty: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaterLevelSnapshot {
    pub sensors: [WaterLevelReading; WaterLevelRole::COUNT],
}

impl WaterLevelSnapshot {
    pub fn dry() -> Self {
        Self {
            sensors: WaterLevelRole::ALL.map(|role| WaterLevelReading {
                role,
                state: WaterLevelState::Dry,
                faulty: false,
            }),
        }
    }

    pub fn get(&self, role: WaterLevelRole) -> WaterLevelReading {
        self.sensors[role.index()]
    }

    pub fn all_healthy(&self) -> bool {
        self.sensors.iter().all(|reading| !reading.faulty)
    }
}
