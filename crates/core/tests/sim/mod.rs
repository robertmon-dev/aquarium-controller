#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use aquarium_core::adapters::outbound::memory::{InMemoryWaterLevelStore, VolatilePumpStore};
use aquarium_core::application::{
    pump_loop, PumpController, PumpService, PumpTiming, WaterLevelSampler,
};
use aquarium_core::domain::cron::CivilTime;
use aquarium_core::domain::pump::{PumpId, PumpSettings, PumpsStatus};
use aquarium_core::domain::water::{
    WaterLevelReading, WaterLevelRole, WaterLevelSnapshot, WaterLevelState,
};
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::{
    Clock, PumpOutputs, PumpRecord, PumpStore, WaterLevelPins, WaterLevelSink, WaterLevelSource,
};
use aquarium_core::protocol::xkc_y25::{FilterConfig, Polarity};

use crate::common::{body, get, raw, serve_pump_control, TOKEN};

pub const STEP: Duration = Duration::from_millis(100);
pub const COARSE_STEP: Duration = Duration::from_secs(1);
pub const BASE_EPOCH: u64 = 1_795_341_480;

pub const TIMING: PumpTiming = PumpTiming {
    return_restart_delay: Duration::from_secs(30),
    level_stale: Duration::from_secs(1),
    tick: Duration::from_secs(1),
    checkpoint_interval: Duration::from_secs(10),
    runtime_flush: Duration::from_secs(600),
};

pub const COARSE_TIMING: PumpTiming = PumpTiming {
    level_stale: Duration::from_secs(3),
    ..TIMING
};

pub const FIRMWARE_FILTER: FilterConfig = FilterConfig {
    debounce_samples: 10,
    flap_window_samples: 300,
    flap_max_transitions: 60,
};

#[derive(Debug, Clone)]
pub struct Lcg(u64);

impl Lcg {
    pub const fn new(seed: u64) -> Self {
        Self(seed ^ 0x9E37_79B9_7F4A_7C15)
    }

    pub fn next_u32(&mut self) -> u32 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.0 >> 32) as u32
    }

    pub fn unit(&mut self) -> f64 {
        f64::from(self.next_u32()) / (f64::from(u32::MAX) + 1.0)
    }

    pub fn range(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.unit()
    }

    pub fn chance(&mut self, probability: f64) -> bool {
        self.unit() < probability
    }

    pub fn coin(&mut self) -> bool {
        self.next_u32() >> 31 == 1
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Mount {
    pub chamber: usize,
    pub height_cm: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub tank_weir_l: f64,
    pub tank_max_l: f64,
    pub weir_skim_l: f64,
    pub drain_intake_l: f64,
    pub chamber_widths_cm: [f64; 5],
    pub chamber_depth_cm: f64,
    pub sump_height_cm: f64,
    pub baffle_cm: f64,
    pub return_chamber: usize,
    pub return_operating_cm: f64,
    pub return_intake_cm: f64,
    pub sump_pump: Mount,
    pub sump_biological: Mount,
    pub sump_mechanical: Mount,
    pub sump_return_level: Mount,
    pub main_tank_margin_l: f64,
    pub reservoir_l: f64,
    pub reserve_wet_l: f64,
    pub reservoir_ready_l: f64,
    pub reservoir_full_l: f64,
    pub return_lph: f64,
    pub drain_lph: f64,
    pub makeup_lph: f64,
}

impl Layout {
    pub const REAL: Self = Self {
        tank_weir_l: 700.0,
        tank_max_l: 720.0,
        weir_skim_l: 3.0,
        drain_intake_l: 560.0,
        chamber_widths_cm: [27.0, 27.0, 27.0, 27.0, 12.0],
        chamber_depth_cm: 43.0,
        sump_height_cm: 40.0,
        baffle_cm: 30.0,
        return_chamber: 4,
        return_operating_cm: 25.0,
        return_intake_cm: 3.0,
        sump_pump: Mount {
            chamber: 4,
            height_cm: 8.0,
        },
        sump_biological: Mount {
            chamber: 2,
            height_cm: 35.0,
        },
        sump_mechanical: Mount {
            chamber: 1,
            height_cm: 36.0,
        },
        sump_return_level: Mount {
            chamber: 4,
            height_cm: 25.0,
        },
        main_tank_margin_l: 2.0,
        reservoir_l: 100.0,
        reserve_wet_l: 3.0,
        reservoir_ready_l: 50.0,
        reservoir_full_l: 95.0,
        return_lph: 3000.0,
        drain_lph: 1500.0,
        makeup_lph: 600.0,
    };

    pub const fn wide_return_chamber(self) -> Self {
        Self {
            chamber_widths_cm: [12.0, 27.0, 27.0, 27.0, 27.0],
            ..self
        }
    }

    pub const fn with_reservoir(self, litres: f64) -> Self {
        Self {
            reservoir_l: litres,
            ..self
        }
    }

    fn area(&self, chamber: usize) -> f64 {
        self.chamber_widths_cm[chamber] * self.chamber_depth_cm / 1000.0
    }

    pub fn return_area_l_per_cm(&self) -> f64 {
        self.area(self.return_chamber)
    }

    pub fn baffled_area_l_per_cm(&self) -> f64 {
        (0..self.chamber_widths_cm.len())
            .filter(|&chamber| chamber != self.return_chamber)
            .map(|chamber| self.area(chamber))
            .sum()
    }

    pub fn total_area_l_per_cm(&self) -> f64 {
        self.return_area_l_per_cm() + self.baffled_area_l_per_cm()
    }

    pub fn baffled_capacity_l(&self) -> f64 {
        self.baffled_area_l_per_cm() * self.baffle_cm
    }

    pub fn full_system_l(&self) -> f64 {
        self.tank_weir_l
            + self.baffled_capacity_l()
            + self.return_operating_cm * self.return_area_l_per_cm()
    }

    pub fn buffer_to_sump_pump_l(&self) -> f64 {
        (self.return_operating_cm - self.sump_pump.height_cm) * self.return_area_l_per_cm()
    }

    pub fn buffer_to_return_intake_l(&self) -> f64 {
        (self.return_operating_cm - self.return_intake_cm) * self.return_area_l_per_cm()
    }

    pub fn headroom_to_baffle_l(&self) -> f64 {
        (self.baffle_cm - self.return_operating_cm) * self.return_area_l_per_cm()
    }

    pub fn headroom_to_level_l(&self, height_cm: f64) -> f64 {
        self.headroom_to_baffle_l() + (height_cm - self.baffle_cm) * self.total_area_l_per_cm()
    }

    pub fn return_level_with_l(&self, extra_l: f64) -> f64 {
        if extra_l <= self.headroom_to_baffle_l() {
            self.return_operating_cm + extra_l / self.return_area_l_per_cm()
        } else {
            self.baffle_cm + (extra_l - self.headroom_to_baffle_l()) / self.total_area_l_per_cm()
        }
    }

    pub fn headroom_to_rim_l(&self) -> f64 {
        self.headroom_to_level_l(self.sump_height_cm)
    }

    pub fn drain_l_per_s(&self) -> f64 {
        self.drain_lph / 3600.0
    }

    pub fn makeup_l_per_s(&self) -> f64 {
        self.makeup_lph / 3600.0
    }

    pub fn return_l_per_s(&self) -> f64 {
        self.return_lph / 3600.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    StuckWet,
    StuckDry,
    Faulty,
    Flapping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Pump(PumpId, bool),
    Sensor(WaterLevelRole, bool),
}

pub struct Aquarium {
    pub layout: Layout,
    pub tank_l: f64,
    pub baffled_l: f64,
    pub return_l: f64,
    pub reservoir_l: f64,
    pub pumps: [bool; 4],
    pub weir_blocked: bool,
    pub faults: [Option<Fault>; WaterLevelRole::COUNT],
    pub outage: bool,
    pub sensing: bool,
    pub boots: u32,
    pub wave_cm: f64,
    pub drained_l: f64,
    pub added_l: f64,
    pub evaporated_l: f64,
    pub supplied_l: f64,
    pub spilled_l: f64,
    pub tank_spilled_l: f64,
    pub return_dry_s: f64,
    pub makeup_dry_s: f64,
    pub drain_dry_s: f64,
    pub solute_mg: f64,
    pub makeup_mg_l: f64,
    pub t_s: f64,
    pub log: Vec<(f64, Event)>,
    rng: Lcg,
    wet: [Option<bool>; WaterLevelRole::COUNT],
    initial_l: f64,
}

impl Aquarium {
    pub fn new(layout: Layout) -> Self {
        let mut aquarium = Self {
            layout,
            tank_l: layout.tank_weir_l,
            baffled_l: layout.baffled_capacity_l(),
            return_l: layout.return_operating_cm * layout.return_area_l_per_cm(),
            reservoir_l: layout.reservoir_l,
            pumps: PumpId::ALL.map(PumpId::fail_safe_on),
            weir_blocked: false,
            faults: [None; WaterLevelRole::COUNT],
            outage: false,
            sensing: true,
            boots: 0,
            wave_cm: 0.0,
            drained_l: 0.0,
            added_l: 0.0,
            evaporated_l: 0.0,
            supplied_l: 0.0,
            spilled_l: 0.0,
            tank_spilled_l: 0.0,
            return_dry_s: 0.0,
            makeup_dry_s: 0.0,
            drain_dry_s: 0.0,
            solute_mg: 0.0,
            makeup_mg_l: 0.0,
            t_s: 0.0,
            log: Vec::new(),
            rng: Lcg::new(1),
            wet: [None; WaterLevelRole::COUNT],
            initial_l: 0.0,
        };
        aquarium.initial_l = aquarium.stored_l();
        aquarium.track_sensors();
        aquarium
    }

    pub fn seed(&mut self, seed: u64) {
        self.rng = Lcg::new(seed);
    }

    pub fn stored_l(&self) -> f64 {
        self.tank_l + self.sump_l() + self.reservoir_l
    }

    pub fn balance_error_l(&self) -> f64 {
        self.stored_l() + self.drained_l + self.evaporated_l + self.spilled_l + self.tank_spilled_l
            - self.supplied_l
            - self.initial_l
    }

    pub fn sump_l(&self) -> f64 {
        self.baffled_l + self.return_l
    }

    pub fn system_l(&self) -> f64 {
        self.tank_l + self.sump_l()
    }

    pub fn deficit_l(&self) -> f64 {
        self.layout.full_system_l() - self.system_l()
    }

    pub fn concentration_mg_l(&self) -> f64 {
        self.solute_mg / self.system_l()
    }

    pub fn dissolve(&mut self, mg_per_l: f64) {
        self.solute_mg = mg_per_l * self.system_l();
    }

    pub fn return_level_cm(&self) -> f64 {
        let layout = &self.layout;
        let below_baffle = layout.return_area_l_per_cm() * layout.baffle_cm;

        if self.return_l <= below_baffle {
            self.return_l / layout.return_area_l_per_cm()
        } else {
            layout.baffle_cm + (self.return_l - below_baffle) / layout.total_area_l_per_cm()
        }
    }

    pub fn baffled_level_cm(&self) -> f64 {
        let level = self.return_level_cm();

        if level > self.layout.baffle_cm {
            level
        } else {
            self.baffled_l / self.layout.baffled_area_l_per_cm()
        }
    }

    fn level_at(&self, chamber: usize) -> f64 {
        if chamber == self.layout.return_chamber {
            self.return_level_cm()
        } else {
            self.baffled_level_cm()
        }
    }

    pub fn evaporate(&mut self, litres: f64) {
        self.return_l -= litres;
        self.evaporated_l += litres;
        self.track_sensors();
    }

    pub fn evaporate_tank(&mut self, litres: f64) {
        self.tank_l -= litres;
        self.evaporated_l += litres;
        self.track_sensors();
    }

    pub fn refill_reservoir(&mut self, to_l: f64) -> f64 {
        let added = (to_l - self.reservoir_l).max(0.0);
        self.reservoir_l += added;
        self.supplied_l += added;
        self.track_sensors();
        added
    }

    pub fn pump_on(&self, id: PumpId) -> bool {
        self.pumps[id.index()]
    }

    pub fn set_pump(&mut self, id: PumpId, on: bool) {
        if self.pumps[id.index()] != on {
            self.pumps[id.index()] = on;
            self.log.push((self.t_s, Event::Pump(id, on)));

            if id == PumpId::Return
                && !on
                && !self.weir_blocked
                && self.tank_l >= self.layout.tank_weir_l
            {
                self.tank_l -= self.layout.weir_skim_l;
                self.return_l += self.layout.weir_skim_l;
                self.settle_sump();
                self.track_sensors();
            }
        }
    }

    pub fn fail_safe(&mut self) {
        for id in PumpId::ALL {
            self.set_pump(id, id.fail_safe_on());
        }
    }

    pub fn power_off(&mut self) {
        for id in PumpId::ALL {
            self.set_pump(id, false);
        }
    }

    pub fn first_after(&self, event: Event, after_s: f64) -> Option<f64> {
        self.log
            .iter()
            .find(|(at, logged)| *logged == event && *at >= after_s)
            .map(|(at, _)| *at)
    }

    pub fn count_after(&self, event: Event, after_s: f64) -> usize {
        self.log
            .iter()
            .filter(|(at, logged)| *logged == event && *at >= after_s)
            .count()
    }

    pub fn count(&self, event: Event) -> usize {
        self.log
            .iter()
            .filter(|(_, logged)| *logged == event)
            .count()
    }

    pub fn runs(&self, id: PumpId, after_s: f64) -> Vec<(f64, f64)> {
        let mut runs = Vec::new();
        let mut started = None;

        for &(at, event) in &self.log {
            match event {
                Event::Pump(pump, true) if pump == id => started = Some(at),
                Event::Pump(pump, false) if pump == id => {
                    if let Some(from) = started.take() {
                        if from >= after_s {
                            runs.push((from, at));
                        }
                    }
                }
                _ => {}
            }
        }

        runs
    }

    pub fn step(&mut self, dt: f64) {
        let layout = self.layout;
        self.t_s += dt;

        if self.pump_on(PumpId::Makeup) {
            let capacity = layout.makeup_l_per_s() * dt;
            let moved = capacity.min(self.reservoir_l.max(0.0));
            self.reservoir_l -= moved;
            self.return_l += moved;
            self.added_l += moved;
            self.solute_mg += moved * self.makeup_mg_l;
            self.makeup_dry_s += dt * (1.0 - moved / capacity);
        }

        if self.pump_on(PumpId::Return) {
            let capacity = layout.return_l_per_s() * dt;
            let intake_l = layout.return_intake_cm * layout.return_area_l_per_cm();
            let moved = capacity.min((self.return_l - intake_l).max(0.0));
            self.return_l -= moved;
            self.tank_l += moved;
            self.return_dry_s += dt * (1.0 - moved / capacity);
        }

        if self.pump_on(PumpId::Drain) {
            let capacity = layout.drain_l_per_s() * dt;
            let moved = capacity.min((self.tank_l - layout.drain_intake_l).max(0.0));
            self.solute_mg -= moved * self.concentration_mg_l();
            self.tank_l -= moved;
            self.drained_l += moved;
            self.drain_dry_s += dt * (1.0 - moved / capacity);
        }

        if self.weir_blocked {
            if self.tank_l > layout.tank_max_l {
                let excess = self.tank_l - layout.tank_max_l;
                self.solute_mg -= excess * self.concentration_mg_l();
                self.tank_spilled_l += excess;
                self.tank_l = layout.tank_max_l;
            }
        } else if self.tank_l > layout.tank_weir_l {
            let excess = self.tank_l - layout.tank_weir_l;
            self.tank_l = layout.tank_weir_l;
            let into_baffled = excess.min(layout.baffled_capacity_l() - self.baffled_l);
            self.baffled_l += into_baffled;
            self.return_l += excess - into_baffled;
        }

        self.settle_sump();
        self.track_sensors();
    }

    fn settle_sump(&mut self) {
        let layout = self.layout;
        let below_baffle = layout.return_area_l_per_cm() * layout.baffle_cm;
        if self.return_l > below_baffle && self.baffled_l < layout.baffled_capacity_l() {
            let back =
                (self.return_l - below_baffle).min(layout.baffled_capacity_l() - self.baffled_l);
            self.return_l -= back;
            self.baffled_l += back;
        }

        let level = self.return_level_cm();
        if level > layout.sump_height_cm {
            let excess = (level - layout.sump_height_cm) * layout.total_area_l_per_cm();
            self.solute_mg -= excess * self.concentration_mg_l();
            self.return_l -= excess;
            self.spilled_l += excess;
        }
    }

    fn mount(&self, role: WaterLevelRole) -> Option<Mount> {
        let layout = &self.layout;

        match role {
            WaterLevelRole::SumpPump => Some(layout.sump_pump),
            WaterLevelRole::SumpBiological => Some(layout.sump_biological),
            WaterLevelRole::SumpMechanical => Some(layout.sump_mechanical),
            WaterLevelRole::SumpReturnLevel => Some(layout.sump_return_level),
            _ => None,
        }
    }

    pub fn surface_distance_cm(&self, role: WaterLevelRole) -> Option<f64> {
        self.mount(role)
            .map(|mount| self.level_at(mount.chamber) - mount.height_cm)
    }

    fn truth(&self, role: WaterLevelRole) -> bool {
        let layout = &self.layout;

        if let Some(mount) = self.mount(role) {
            return self.level_at(mount.chamber) >= mount.height_cm;
        }

        match role {
            WaterLevelRole::MainTank => {
                self.tank_l > layout.tank_weir_l + layout.main_tank_margin_l
            }
            WaterLevelRole::SumpReserve => self.reservoir_l >= layout.reserve_wet_l,
            WaterLevelRole::ReservoirReady => self.reservoir_l >= layout.reservoir_ready_l,
            WaterLevelRole::ReservoirFull => self.reservoir_l >= layout.reservoir_full_l,
            _ => unreachable!("mounted roles are handled above"),
        }
    }

    pub fn raw_wet(&mut self, role: WaterLevelRole) -> bool {
        match self.faults[role.index()] {
            Some(Fault::StuckWet) => true,
            Some(Fault::StuckDry) => false,
            Some(Fault::Flapping) => self.rng.coin(),
            Some(Fault::Faulty) | None => {
                let in_waves = self
                    .surface_distance_cm(role)
                    .is_some_and(|distance| distance.abs() < self.wave_cm);

                if in_waves {
                    self.rng.coin()
                } else {
                    self.truth(role)
                }
            }
        }
    }

    fn reading(&self, role: WaterLevelRole) -> WaterLevelReading {
        let fault = self.faults[role.index()];
        let wet = match fault {
            Some(Fault::StuckWet) => true,
            Some(Fault::StuckDry) => false,
            Some(Fault::Faulty | Fault::Flapping) | None => self.truth(role),
        };

        WaterLevelReading {
            role,
            state: state(wet),
            faulty: fault == Some(Fault::Faulty),
        }
    }

    fn track_sensors(&mut self) {
        for role in WaterLevelRole::ALL {
            let wet = self.reading(role).state == WaterLevelState::Wet;

            if self.wet[role.index()] != Some(wet) {
                if self.wet[role.index()].is_some() {
                    self.log.push((self.t_s, Event::Sensor(role, wet)));
                }
                self.wet[role.index()] = Some(wet);
            }
        }
    }

    pub fn snapshot(&self) -> WaterLevelSnapshot {
        WaterLevelSnapshot {
            sensors: WaterLevelRole::ALL.map(|role| self.reading(role)),
        }
    }

    pub fn raw_snapshot(&mut self) -> WaterLevelSnapshot {
        WaterLevelSnapshot {
            sensors: WaterLevelRole::ALL.map(|role| WaterLevelReading {
                role,
                state: state(self.raw_wet(role)),
                faulty: self.faults[role.index()] == Some(Fault::Faulty),
            }),
        }
    }

    pub fn describe(&self) -> String {
        format!(
            "t={:.1}s tank={:.2}L return={:.2}cm baffled={:.2}cm reservoir={:.2}L pumps={:?} \
             drained={:.2}L added={:.2}L spilled={:.2}L tank_spilled={:.2}L \
             return_dry={:.1}s makeup_dry={:.1}s drain_dry={:.1}s",
            self.t_s,
            self.tank_l,
            self.return_level_cm(),
            self.baffled_level_cm(),
            self.reservoir_l,
            self.pumps,
            self.drained_l,
            self.added_l,
            self.spilled_l,
            self.tank_spilled_l,
            self.return_dry_s,
            self.makeup_dry_s,
            self.drain_dry_s,
        )
    }
}

fn state(wet: bool) -> WaterLevelState {
    if wet {
        WaterLevelState::Wet
    } else {
        WaterLevelState::Dry
    }
}

pub type Water = Arc<Mutex<Aquarium>>;

pub struct SimOutputs(Water);

impl PumpOutputs for SimOutputs {
    fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
        self.0.lock().expect("aquarium poisoned").set_pump(id, on);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zone {
    Utc,
    Warsaw,
}

impl Zone {
    pub fn offset_s(self, epoch: u64) -> u64 {
        match self {
            Self::Utc => 0,
            Self::Warsaw if summer_time(epoch) => 7200,
            Self::Warsaw => 3600,
        }
    }

    pub fn local(self, epoch: u64) -> u64 {
        epoch + self.offset_s(epoch)
    }
}

pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(month);
    let shifted = if month > 2 { month - 3 } else { month + 9 };
    let doy = (153 * shifted + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn ymd(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (
        yoe + era * 400 + i64::from(month <= 2),
        month as u32,
        day as u32,
    )
}

fn last_sunday(year: i64, month: u32) -> i64 {
    let last = days_from_civil(year, month, 31);
    last - (last + 4).rem_euclid(7)
}

fn summer_time(epoch: u64) -> bool {
    let (year, _, _) = ymd((epoch / 86_400) as i64);
    let start = last_sunday(year, 3) * 86_400 + 3600;
    let end = last_sunday(year, 10) * 86_400 + 3600;
    (start..end).contains(&(epoch as i64))
}

pub fn utc(year: i64, month: u32, day: u32, hour: u64, minute: u64) -> u64 {
    days_from_civil(year, month, day) as u64 * 86_400 + hour * 3600 + minute * 60
}

pub fn civil(epoch: u64) -> CivilTime {
    let days = (epoch / 86_400) as i64;
    let seconds = epoch % 86_400;
    let (_, month, day) = ymd(days);

    CivilTime {
        minute: ((seconds / 60) % 60) as u8,
        hour: (seconds / 3600) as u8,
        day: day as u8,
        month: month as u8,
        weekday: (days + 4).rem_euclid(7) as u8,
    }
}

pub struct SimClock {
    origin: Instant,
    base: u64,
    zone: Zone,
    boot: Mutex<Instant>,
    synced: AtomicBool,
}

impl SimClock {
    pub fn new() -> Self {
        Self::starting(BASE_EPOCH, Zone::Utc)
    }

    pub fn starting(base: u64, zone: Zone) -> Self {
        let now = Instant::now();
        Self {
            origin: now,
            base,
            zone,
            boot: Mutex::new(now),
            synced: AtomicBool::new(true),
        }
    }

    pub fn reboot(&self, synced: bool) {
        *self.boot.lock().expect("clock poisoned") = Instant::now();
        self.synced.store(synced, Ordering::Relaxed);
    }

    pub fn sync(&self) {
        self.synced.store(true, Ordering::Relaxed);
    }

    pub fn lose_sync(&self) {
        self.synced.store(false, Ordering::Relaxed);
    }

    pub fn is_synced(&self) -> bool {
        self.synced.load(Ordering::Relaxed)
    }

    pub fn true_epoch(&self) -> u64 {
        self.base + self.origin.elapsed().as_secs()
    }

    pub fn true_local(&self) -> CivilTime {
        civil(self.zone.local(self.true_epoch()))
    }
}

impl Clock for SimClock {
    fn epoch_seconds(&self) -> Option<u64> {
        self.is_synced().then(|| self.true_epoch())
    }

    fn uptime_seconds(&self) -> u64 {
        self.boot
            .lock()
            .expect("clock poisoned")
            .elapsed()
            .as_secs()
    }

    fn local_minute_of_day(&self) -> Option<u16> {
        self.local_civil_time()
            .map(|at| u16::from(at.hour) * 60 + u16::from(at.minute))
    }

    fn local_month(&self) -> Option<u8> {
        self.local_civil_time().map(|at| at.month)
    }

    fn local_civil_time(&self) -> Option<CivilTime> {
        self.epoch_seconds()
            .map(|epoch| civil(self.zone.local(epoch)))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StoreOp {
    pub record: PumpRecord,
    pub erase: bool,
    pub applied: bool,
    pub tombstone: bool,
}

#[derive(Default)]
pub struct SimStore {
    inner: VolatilePumpStore,
    budget: Mutex<Option<usize>>,
    frozen: AtomicBool,
    ops: Mutex<Vec<StoreOp>>,
}

impl SimStore {
    pub fn fail_after(&self, ops: usize) {
        *self.budget.lock().expect("store poisoned") = Some(ops);
    }

    pub fn frozen(&self) -> bool {
        self.frozen.load(Ordering::Relaxed)
    }

    pub fn thaw(&self) {
        *self.budget.lock().expect("store poisoned") = None;
        self.frozen.store(false, Ordering::Relaxed);
    }

    pub fn ops(&self) -> Vec<StoreOp> {
        self.ops.lock().expect("store poisoned").clone()
    }

    pub fn op_count(&self) -> usize {
        self.ops.lock().expect("store poisoned").len()
    }

    pub fn get(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.inner.get(record)
    }

    pub fn holds_checkpoint(&self) -> bool {
        self.inner
            .get(PumpRecord::Checkpoint)
            .is_some_and(|bytes| bytes != b"null")
    }

    fn admit(&self, record: PumpRecord, erase: bool, tombstone: bool) -> bool {
        let mut ops = self.ops.lock().expect("store poisoned");
        let within = self
            .budget
            .lock()
            .expect("store poisoned")
            .is_none_or(|budget| ops.len() < budget);
        let applied = within && !self.frozen();

        if !applied {
            self.frozen.store(true, Ordering::Relaxed);
        }

        ops.push(StoreOp {
            record,
            erase,
            applied,
            tombstone,
        });
        applied
    }
}

impl PumpStore for SimStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.inner.load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        if self.admit(record, false, bytes == b"null") {
            self.inner.store(record, bytes)
        } else {
            Ok(())
        }
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        if self.admit(record, true, false) {
            self.inner.erase(record)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum Sensors {
    Raw,
    Filtered(FilterConfig),
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    pub step: Duration,
    pub timing: PumpTiming,
    pub epoch: u64,
    pub zone: Zone,
    pub sensors: Sensors,
    pub driver: bool,
    pub http: bool,
}

impl Options {
    pub const DEFAULT: Self = Self {
        step: STEP,
        timing: TIMING,
        epoch: BASE_EPOCH,
        zone: Zone::Utc,
        sensors: Sensors::Raw,
        driver: true,
        http: true,
    };

    pub const COARSE: Self = Self {
        step: COARSE_STEP,
        timing: COARSE_TIMING,
        ..Self::DEFAULT
    };

    pub const fn manual(self) -> Self {
        Self {
            driver: false,
            http: false,
            ..self
        }
    }
}

struct SimPins(Water);

impl WaterLevelPins for SimPins {
    fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT] {
        let mut aquarium = self.0.lock().expect("aquarium poisoned");
        WaterLevelRole::ALL.map(|role| (role, !aquarium.raw_wet(role)))
    }
}

type Sampler = WaterLevelSampler<SimPins, Arc<InMemoryWaterLevelStore>>;

struct Publisher {
    water: Water,
    levels: Arc<InMemoryWaterLevelStore>,
    sensors: Sensors,
    sampler: Option<(u32, Sampler)>,
}

impl Publisher {
    fn advance(&mut self, dt: f64) {
        self.water.lock().expect("aquarium poisoned").step(dt);
        self.publish();
    }

    fn publish(&mut self) {
        let boots = {
            let aquarium = self.water.lock().expect("aquarium poisoned");
            if aquarium.outage || !aquarium.sensing {
                return;
            }
            aquarium.boots
        };

        match self.sensors {
            Sensors::Raw => {
                let snapshot = self.water.lock().expect("aquarium poisoned").raw_snapshot();
                self.levels.publish(snapshot);
            }
            Sensors::Filtered(filter) => {
                if self.sampler.as_ref().is_none_or(|(seen, _)| *seen != boots) {
                    self.sampler = Some((
                        boots,
                        WaterLevelSampler::new(
                            SimPins(Arc::clone(&self.water)),
                            Arc::clone(&self.levels),
                            Polarity::ActiveLow,
                            filter,
                        ),
                    ));
                }

                if let Some((_, sampler)) = &mut self.sampler {
                    sampler.tick();
                }
            }
        }
    }
}

fn spawn_driver(mut publisher: Publisher, step: Duration) {
    tokio::task::spawn_local(async move {
        let mut ticker = tokio::time::interval(step);
        ticker.tick().await;
        publisher.publish();

        loop {
            ticker.tick().await;
            publisher.advance(step.as_secs_f64());
        }
    });
}

pub type Service = PumpService<Arc<SimStore>, Arc<SimClock>>;

pub struct Sim {
    pub water: Water,
    pub clock: Arc<SimClock>,
    pub store: Arc<SimStore>,
    pub service: Arc<Service>,
    pub addr: SocketAddr,
    pub options: Options,
    levels: Arc<InMemoryWaterLevelStore>,
    manual: Option<Mutex<Publisher>>,
    control: JoinHandle<()>,
    server: Option<JoinHandle<()>>,
}

struct Booted {
    service: Arc<Service>,
    control: JoinHandle<()>,
    addr: SocketAddr,
    server: Option<JoinHandle<()>>,
}

async fn boot(
    water: &Water,
    clock: &Arc<SimClock>,
    store: &Arc<SimStore>,
    levels: &Arc<InMemoryWaterLevelStore>,
    options: &Options,
) -> Booted {
    let service = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        Arc::clone(store),
        Arc::clone(clock),
        None,
    ));

    let control = tokio::task::spawn_local(pump_loop(
        PumpController::new(
            SimOutputs(Arc::clone(water)),
            Arc::clone(&service),
            options.timing,
        ),
        levels.subscribe(),
        service.change_signal(),
        Arc::new(Notify::new()),
        options.timing,
        Arc::new(AtomicU32::new(0)),
    ));

    let (addr, server) = if options.http {
        let (addr, server) = serve_pump_control(Arc::clone(&service)).await;
        (addr, Some(server))
    } else {
        (SocketAddr::from(([127, 0, 0, 1], 0)), None)
    };

    Booted {
        service,
        control,
        addr,
        server,
    }
}

fn code(response: &str) -> u16 {
    response
        .get(9..12)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status line in: {response}"))
}

impl Sim {
    pub async fn start(layout: Layout) -> Self {
        Self::start_with(layout, Options::DEFAULT).await
    }

    pub async fn start_with(layout: Layout, options: Options) -> Self {
        Self::start_on(Aquarium::new(layout), options).await
    }

    pub async fn start_on(aquarium: Aquarium, options: Options) -> Self {
        let water = Arc::new(Mutex::new(aquarium));
        let levels = Arc::new(InMemoryWaterLevelStore::new());
        let clock = Arc::new(SimClock::starting(options.epoch, options.zone));
        let store = Arc::new(SimStore::default());

        let mut publisher = Publisher {
            water: Arc::clone(&water),
            levels: Arc::clone(&levels),
            sensors: options.sensors,
            sampler: None,
        };

        let manual = if options.driver {
            spawn_driver(publisher, options.step);
            None
        } else {
            publisher.publish();
            Some(Mutex::new(publisher))
        };

        let booted = boot(&water, &clock, &store, &levels, &options).await;

        Self {
            water,
            clock,
            store,
            service: booted.service,
            addr: booted.addr,
            options,
            levels,
            manual,
            control: booted.control,
            server: booted.server,
        }
    }

    pub fn water(&self) -> MutexGuard<'_, Aquarium> {
        self.water.lock().expect("aquarium poisoned")
    }

    pub fn published(&self) -> WaterLevelSnapshot {
        self.levels.levels()
    }

    pub fn crash(&mut self, power_lost: bool) {
        self.control.abort();
        if let Some(server) = self.server.take() {
            server.abort();
        }

        let mut water = self.water();
        water.sensing = false;
        if power_lost {
            water.power_off();
        } else {
            water.fail_safe();
        }
    }

    pub async fn boot_up(&mut self, synced: bool) {
        {
            let mut water = self.water();
            water.fail_safe();
            water.sensing = true;
            water.boots += 1;
        }
        self.clock.reboot(synced);
        self.store.thaw();

        let booted = boot(
            &self.water,
            &self.clock,
            &self.store,
            &self.levels,
            &self.options,
        )
        .await;
        self.service = booted.service;
        self.control = booted.control;
        self.addr = booted.addr;
        self.server = booted.server;
    }

    pub async fn restart(&mut self, outage: Duration, synced: bool) {
        self.crash(false);
        self.run_for(outage).await;
        self.boot_up(synced).await;
    }

    pub async fn power_cut(&mut self, outage: Duration, synced: bool) {
        self.crash(true);
        self.run_for(outage).await;
        self.boot_up(synced).await;
    }

    pub async fn step(&self) {
        tokio::time::sleep(self.options.step).await;

        if let Some(publisher) = &self.manual {
            publisher
                .lock()
                .expect("publisher poisoned")
                .advance(self.options.step.as_secs_f64());
        }
    }

    pub async fn run_for(&self, duration: Duration) {
        if self.manual.is_none() {
            tokio::time::sleep(duration).await;
            return;
        }

        let deadline = Instant::now() + duration;
        while Instant::now() + self.options.step <= deadline {
            self.step().await;
        }
    }

    pub async fn run_until(
        &self,
        limit: Duration,
        what: &str,
        mut done: impl FnMut(&PumpsStatus, &Aquarium) -> bool,
    ) -> f64 {
        let deadline = Instant::now() + limit;

        loop {
            {
                let status = self.service.status();
                let water = self.water();
                if done(&status, &water) {
                    return water.t_s;
                }

                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for {what}: {} / {status:?}",
                    water.describe()
                );
            }

            if self.manual.is_some() {
                self.step().await;
            } else {
                tokio::time::sleep(STEP.min(self.options.step)).await;
            }
        }
    }

    pub async fn send(&self, method: &str, path: &str, json: &str) -> (u16, String) {
        let response = raw(self.addr, &body(method, path, Some(TOKEN), json)).await;
        (code(&response), response)
    }

    pub async fn fetch<T: DeserializeOwned>(&self, path: &str) -> T {
        let response = raw(self.addr, &get(path, Some(TOKEN))).await;
        assert_eq!(code(&response), 200, "got: {response}");
        let (_, payload) = response.split_once("\r\n\r\n").expect("no body");
        serde_json::from_str(payload.trim())
            .unwrap_or_else(|error| panic!("unexpected body ({error}): {payload}"))
    }

    pub async fn status(&self) -> PumpsStatus {
        self.fetch("/api/pumps").await
    }
}
