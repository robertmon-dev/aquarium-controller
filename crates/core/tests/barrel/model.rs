use aquarium_core::domain::reservoir::ReservoirChannel;
use aquarium_core::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

#[derive(Debug, Clone, Copy)]
pub struct BarrelLayout {
    pub capacity_l: f64,
    pub reserve_l: f64,
    pub ready_l: f64,
    pub full_l: f64,
    pub ro_lph: f64,
    pub tap_lph: f64,
    pub ro_ppm: f64,
    pub tap_ppm: f64,
    pub mixed_tau_s: f64,
    pub still_tau_s: f64,
}

impl BarrelLayout {
    pub const DEFAULT: Self = Self {
        capacity_l: 120.0,
        reserve_l: 10.0,
        ready_l: 50.0,
        full_l: 100.0,
        ro_lph: 60.0,
        tap_lph: 600.0,
        ro_ppm: 5.0,
        tap_ppm: 300.0,
        mixed_tau_s: 5.0,
        still_tau_s: 900.0,
    };

    pub fn empty_fill_s(&self) -> u32 {
        (self.full_l / self.ro_lph * 3600.0).ceil() as u32
    }
}

use ReservoirChannel::{Booster, Mixer, RoValve, TapValve};

pub struct Barrel {
    pub layout: BarrelLayout,
    pub litres: f64,
    pub salt: f64,
    pub probe_ppm: f64,
    pub valves: [bool; 4],
    pub refuse_off: Option<ReservoirChannel>,
    pub hose_burst: bool,
    pub full_stuck_dry: bool,
    pub poured_l: [f64; 2],
    pub spilled_l: f64,
    pub overflow_l: f64,
    pub poured_during_water_change_l: f64,
    pub water_change_active: bool,
    pub runs: Vec<(ReservoirChannel, bool, f64)>,
    pub t_s: f64,
}

impl Barrel {
    pub fn new(layout: BarrelLayout) -> Self {
        Self {
            layout,
            litres: 0.0,
            salt: 0.0,
            probe_ppm: 0.0,
            valves: [false; 4],
            refuse_off: None,
            hose_burst: false,
            full_stuck_dry: false,
            poured_l: [0.0; 2],
            spilled_l: 0.0,
            overflow_l: 0.0,
            poured_during_water_change_l: 0.0,
            water_change_active: false,
            runs: Vec::new(),
            t_s: 0.0,
        }
    }

    pub fn with_water(mut self, litres: f64, ppm: f64) -> Self {
        self.litres = litres;
        self.salt = litres * ppm;
        self.probe_ppm = ppm;
        self
    }

    pub fn mean_ppm(&self) -> f64 {
        if self.litres <= 0.0 {
            0.0
        } else {
            self.salt / self.litres
        }
    }

    pub fn draw(&mut self, litres: f64) {
        let taken = litres.min(self.litres);
        let ppm = self.mean_ppm();
        self.litres -= taken;
        self.salt -= taken * ppm;
    }

    pub fn add(&mut self, litres: f64, ppm: f64) {
        self.litres += litres;
        self.salt += litres * ppm;
    }

    pub fn step(&mut self, dt: f64) {
        self.t_s += dt;
        let layout = self.layout;
        let ro = self.valves[RoValve.index()] && self.valves[Booster.index()];
        let tap = self.valves[TapValve.index()];
        let inflow = [
            if ro { layout.ro_lph * dt / 3600.0 } else { 0.0 },
            if tap {
                layout.tap_lph * dt / 3600.0
            } else {
                0.0
            },
        ];

        for (index, litres) in inflow.into_iter().enumerate() {
            if litres == 0.0 {
                continue;
            }
            self.poured_l[index] += litres;
            if self.water_change_active {
                self.poured_during_water_change_l += litres;
            }
            if self.hose_burst {
                self.spilled_l += litres;
                continue;
            }
            self.add(
                litres,
                if index == 0 {
                    layout.ro_ppm
                } else {
                    layout.tap_ppm
                },
            );
        }

        if self.litres > layout.capacity_l {
            let extra = self.litres - layout.capacity_l;
            let ppm = self.mean_ppm();
            self.overflow_l += extra;
            self.litres = layout.capacity_l;
            self.salt -= extra * ppm;
        }

        let stirred = self.valves[Mixer.index()] && self.litres >= layout.reserve_l;
        let tau = if stirred {
            layout.mixed_tau_s
        } else {
            layout.still_tau_s
        };
        let mean = self.mean_ppm();
        self.probe_ppm += (mean - self.probe_ppm) * (dt / tau).min(1.0);
    }

    pub fn wet(&self, role: WaterLevelRole) -> bool {
        match role {
            WaterLevelRole::SumpReserve => self.litres >= self.layout.reserve_l,
            WaterLevelRole::ReservoirReady => self.litres >= self.layout.ready_l,
            WaterLevelRole::ReservoirFull => {
                !self.full_stuck_dry && self.litres >= self.layout.full_l
            }
            _ => false,
        }
    }

    pub fn snapshot(&self) -> WaterLevelSnapshot {
        let mut snapshot = WaterLevelSnapshot::dry();
        for reading in &mut snapshot.sensors {
            if self.wet(reading.role) {
                reading.state = WaterLevelState::Wet;
            }
        }
        snapshot.with_plausibility()
    }

    pub fn on_segments(&self, channel: ReservoirChannel) -> Vec<(f64, f64)> {
        let mut segments = Vec::new();
        let mut since = None;
        for &(logged, on, at) in &self.runs {
            if logged != channel {
                continue;
            }
            match (on, since) {
                (true, None) => since = Some(at),
                (false, Some(start)) => {
                    segments.push((start, at));
                    since = None;
                }
                _ => {}
            }
        }
        if let Some(start) = since {
            segments.push((start, self.t_s));
        }
        segments
    }
}
