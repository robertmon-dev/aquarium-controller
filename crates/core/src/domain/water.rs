use core::time::Duration;

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

pub const HARNESS_WINDOW: Duration = Duration::from_secs(3);

const UNPLUGGED: [WaterLevelRole; 2] = [WaterLevelRole::SumpPump, WaterLevelRole::SumpReturnLevel];

const SHORTED: [WaterLevelRole; 3] = [
    WaterLevelRole::MainTank,
    WaterLevelRole::SumpMechanical,
    WaterLevelRole::SumpBiological,
];

#[derive(Debug, Clone, Default)]
pub struct HarnessWatch {
    last: Option<[WaterLevelState; WaterLevelRole::COUNT]>,
    since_snapshot: Duration,
    since_dried: [Option<Duration>; WaterLevelRole::COUNT],
    since_wetted: [Option<Duration>; WaterLevelRole::COUNT],
    unplugged: bool,
    shorted: bool,
}

impl HarnessWatch {
    pub fn update(&mut self, snapshot: Option<&WaterLevelSnapshot>, elapsed: Duration) -> bool {
        for since in self
            .since_dried
            .iter_mut()
            .chain(self.since_wetted.iter_mut())
            .flatten()
        {
            *since = since.saturating_add(elapsed);
        }
        self.since_snapshot = self.since_snapshot.saturating_add(elapsed);

        let Some(snapshot) = snapshot else {
            return self.suspect();
        };

        let states = snapshot.sensors.map(|reading| reading.state);
        let continuous = core::mem::take(&mut self.since_snapshot) <= HARNESS_WINDOW;

        if let Some(last) = self.last.filter(|_| continuous) {
            for role in WaterLevelRole::ALL {
                let slot = role.index();
                match (last[slot], states[slot]) {
                    (WaterLevelState::Wet, WaterLevelState::Dry) => {
                        self.since_dried[slot] = Some(Duration::ZERO);
                    }
                    (WaterLevelState::Dry, WaterLevelState::Wet) => {
                        self.since_wetted[slot] = Some(Duration::ZERO);
                    }
                    _ => {}
                }
            }
        }
        self.last = Some(states);

        let all = |roles: &[WaterLevelRole], state: WaterLevelState| {
            roles.iter().all(|role| states[role.index()] == state)
        };
        let together = |roles: &[WaterLevelRole], since: &[Option<Duration>]| {
            roles
                .iter()
                .all(|role| since[role.index()].is_some_and(|since| since <= HARNESS_WINDOW))
        };

        self.unplugged = all(&UNPLUGGED, WaterLevelState::Dry)
            && (self.unplugged || together(&UNPLUGGED, &self.since_dried));
        self.shorted = all(&SHORTED, WaterLevelState::Wet)
            && (self.shorted || together(&SHORTED, &self.since_wetted));

        self.suspect()
    }

    pub const fn suspect(&self) -> bool {
        self.unplugged || self.shorted
    }
}

#[cfg(test)]
mod tests;
