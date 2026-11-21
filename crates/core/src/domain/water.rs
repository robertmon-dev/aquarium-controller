use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaterLevelState {
    Wet,
    Dry,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export))]
pub enum WaterLevelRole {
    SumpMechanical,
    SumpBiological,
    SumpPump,
    MainTank,
    SumpReserve,
    SumpReturnLevel,
    ReservoirReady,
    ReservoirFull,
}

impl WaterLevelRole {
    pub const COUNT: usize = 8;

    pub const ALL: [Self; Self::COUNT] = [
        Self::SumpMechanical,
        Self::SumpBiological,
        Self::SumpPump,
        Self::MainTank,
        Self::SumpReserve,
        Self::SumpReturnLevel,
        Self::ReservoirReady,
        Self::ReservoirFull,
    ];

    pub const fn index(self) -> usize {
        match self {
            Self::SumpMechanical => 0,
            Self::SumpBiological => 1,
            Self::SumpPump => 2,
            Self::MainTank => 3,
            Self::SumpReserve => 4,
            Self::SumpReturnLevel => 5,
            Self::ReservoirReady => 6,
            Self::ReservoirFull => 7,
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

    pub fn sump_water_low(&self) -> bool {
        self.get(WaterLevelRole::SumpPump).state == WaterLevelState::Dry
    }

    pub fn with_plausibility(mut self) -> Self {
        let raw = self.sensors;

        for (upper, lower) in STACKED {
            let contradicts = raw[upper.index()].state == WaterLevelState::Wet
                && raw[lower.index()].state == WaterLevelState::Dry;

            if contradicts {
                self.sensors[upper.index()].faulty = true;
                self.sensors[lower.index()].faulty = true;
            }
        }

        self
    }
}

const STACKED: [(WaterLevelRole, WaterLevelRole); 3] = [
    (WaterLevelRole::SumpReturnLevel, WaterLevelRole::SumpPump),
    (
        WaterLevelRole::ReservoirFull,
        WaterLevelRole::ReservoirReady,
    ),
    (WaterLevelRole::ReservoirReady, WaterLevelRole::SumpReserve),
];

#[cfg(test)]
mod tests;
