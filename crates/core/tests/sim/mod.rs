use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use aquarium_core::adapters::outbound::memory::{InMemoryWaterLevelStore, VolatilePumpStore};
use aquarium_core::application::{pump_loop, PumpController, PumpService, PumpTiming};
use aquarium_core::domain::cron::CivilTime;
use aquarium_core::domain::pump::{PumpId, PumpSettings, PumpsStatus};
use aquarium_core::domain::water::{
    WaterLevelReading, WaterLevelRole, WaterLevelSnapshot, WaterLevelState,
};
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::{Clock, PumpOutputs, WaterLevelSink};

use crate::common::{body, get, raw, start_with_pump_control, TOKEN};

pub const STEP: Duration = Duration::from_millis(100);
pub const BASE_EPOCH: u64 = 1_795_341_480;

pub const TIMING: PumpTiming = PumpTiming {
    return_restart_delay: Duration::from_secs(30),
    level_stale: Duration::from_secs(1),
    tick: Duration::from_secs(1),
    checkpoint_interval: Duration::from_secs(10),
    runtime_flush: Duration::from_secs(600),
};

#[derive(Debug, Clone, Copy)]
pub struct Mount {
    pub chamber: usize,
    pub height_cm: f64,
}

#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub tank_weir_l: f64,
    pub tank_max_l: f64,
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
    pub main_tank_margin_l: f64,
    pub reservoir_l: f64,
    pub reserve_wet_l: f64,
    pub return_lph: f64,
    pub drain_lph: f64,
    pub makeup_lph: f64,
}

impl Layout {
    pub const REAL: Self = Self {
        tank_weir_l: 700.0,
        tank_max_l: 720.0,
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
        main_tank_margin_l: 2.0,
        reservoir_l: 100.0,
        reserve_wet_l: 3.0,
        return_lph: 3000.0,
        drain_lph: 1500.0,
        makeup_lph: 600.0,
    };

    pub const PROPOSED: Self = Self {
        sump_biological: Mount {
            chamber: 4,
            height_cm: 25.0,
        },
        ..Self::REAL
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

    fn baffled_capacity_l(&self) -> f64 {
        self.baffled_area_l_per_cm() * self.baffle_cm
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
    pub drained_l: f64,
    pub added_l: f64,
    pub spilled_l: f64,
    pub tank_spilled_l: f64,
    pub return_dry_s: f64,
    pub makeup_dry_s: f64,
    pub t_s: f64,
    pub log: Vec<(f64, Event)>,
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
            drained_l: 0.0,
            added_l: 0.0,
            spilled_l: 0.0,
            tank_spilled_l: 0.0,
            return_dry_s: 0.0,
            makeup_dry_s: 0.0,
            t_s: 0.0,
            log: Vec::new(),
            wet: [None; WaterLevelRole::COUNT],
            initial_l: 0.0,
        };
        aquarium.initial_l = aquarium.stored_l();
        aquarium.track_sensors();
        aquarium
    }

    pub fn stored_l(&self) -> f64 {
        self.tank_l + self.sump_l() + self.reservoir_l
    }

    pub fn balance_error_l(&self) -> f64 {
        self.stored_l() + self.drained_l + self.spilled_l + self.tank_spilled_l - self.initial_l
    }

    pub fn sump_l(&self) -> f64 {
        self.baffled_l + self.return_l
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

    pub fn pump_on(&self, id: PumpId) -> bool {
        self.pumps[id.index()]
    }

    pub fn set_pump(&mut self, id: PumpId, on: bool) {
        if self.pumps[id.index()] != on {
            self.pumps[id.index()] = on;
            self.log.push((self.t_s, Event::Pump(id, on)));
        }
    }

    pub fn fail_safe(&mut self) {
        for id in PumpId::ALL {
            self.set_pump(id, id.fail_safe_on());
        }
    }

    pub fn first_after(&self, event: Event, after_s: f64) -> Option<f64> {
        self.log
            .iter()
            .find(|(at, logged)| *logged == event && *at >= after_s)
            .map(|(at, _)| *at)
    }

    pub fn count(&self, event: Event) -> usize {
        self.log
            .iter()
            .filter(|(_, logged)| *logged == event)
            .count()
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
            self.tank_l -= moved;
            self.drained_l += moved;
        }

        if self.weir_blocked {
            if self.tank_l > layout.tank_max_l {
                self.tank_spilled_l += self.tank_l - layout.tank_max_l;
                self.tank_l = layout.tank_max_l;
            }
        } else if self.tank_l > layout.tank_weir_l {
            let excess = self.tank_l - layout.tank_weir_l;
            self.tank_l = layout.tank_weir_l;
            let into_baffled = excess.min(layout.baffled_capacity_l() - self.baffled_l);
            self.baffled_l += into_baffled;
            self.return_l += excess - into_baffled;
        }

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
            self.return_l -= excess;
            self.spilled_l += excess;
        }

        self.track_sensors();
    }

    fn truth(&self, role: WaterLevelRole) -> bool {
        let layout = &self.layout;
        let mounted = |mount: Mount| self.level_at(mount.chamber) >= mount.height_cm;

        match role {
            WaterLevelRole::SumpPump => mounted(layout.sump_pump),
            WaterLevelRole::SumpBiological => mounted(layout.sump_biological),
            WaterLevelRole::SumpMechanical => mounted(layout.sump_mechanical),
            WaterLevelRole::MainTank => {
                self.tank_l > layout.tank_weir_l + layout.main_tank_margin_l
            }
            WaterLevelRole::SumpReserve => self.reservoir_l >= layout.reserve_wet_l,
        }
    }

    fn reading(&self, role: WaterLevelRole) -> WaterLevelReading {
        let fault = self.faults[role.index()];
        let wet = match fault {
            Some(Fault::StuckWet) => true,
            Some(Fault::StuckDry) => false,
            Some(Fault::Faulty) | None => self.truth(role),
        };

        WaterLevelReading {
            role,
            state: if wet {
                WaterLevelState::Wet
            } else {
                WaterLevelState::Dry
            },
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

    pub fn describe(&self) -> String {
        format!(
            "t={:.1}s tank={:.2}L return={:.2}cm baffled={:.2}cm reservoir={:.2}L pumps={:?} \
             drained={:.2}L added={:.2}L spilled={:.2}L tank_spilled={:.2}L \
             return_dry={:.1}s makeup_dry={:.1}s",
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
        )
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

pub struct SimClock {
    origin: Instant,
    boot: Mutex<Instant>,
    synced: AtomicBool,
}

impl SimClock {
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            origin: now,
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
}

pub fn civil(epoch: u64) -> CivilTime {
    let days = (epoch / 86_400) as i64;
    let seconds = epoch % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };

    CivilTime {
        minute: ((seconds / 60) % 60) as u8,
        hour: (seconds / 3600) as u8,
        day: day as u8,
        month: month as u8,
        weekday: (days + 4).rem_euclid(7) as u8,
    }
}

impl Clock for SimClock {
    fn epoch_seconds(&self) -> Option<u64> {
        self.synced
            .load(Ordering::Relaxed)
            .then(|| BASE_EPOCH + self.origin.elapsed().as_secs())
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
        self.epoch_seconds().map(civil)
    }
}

pub type Service = PumpService<Arc<VolatilePumpStore>, Arc<SimClock>>;

pub struct Sim {
    pub water: Water,
    pub clock: Arc<SimClock>,
    pub store: Arc<VolatilePumpStore>,
    pub service: Arc<Service>,
    pub addr: SocketAddr,
    levels: Arc<InMemoryWaterLevelStore>,
    control: JoinHandle<()>,
}

fn spawn_driver(water: Water, levels: Arc<InMemoryWaterLevelStore>) {
    tokio::task::spawn_local(async move {
        let mut ticker = tokio::time::interval(STEP);
        ticker.tick().await;
        levels.publish(water.lock().expect("aquarium poisoned").snapshot());

        loop {
            ticker.tick().await;
            let snapshot = {
                let mut aquarium = water.lock().expect("aquarium poisoned");
                aquarium.step(STEP.as_secs_f64());
                (!aquarium.outage).then(|| aquarium.snapshot())
            };

            if let Some(snapshot) = snapshot {
                levels.publish(snapshot);
            }
        }
    });
}

async fn boot(
    water: &Water,
    clock: &Arc<SimClock>,
    store: &Arc<VolatilePumpStore>,
    levels: &Arc<InMemoryWaterLevelStore>,
) -> (Arc<Service>, JoinHandle<()>, SocketAddr) {
    let service = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        Arc::clone(store),
        Arc::clone(clock),
        None,
    ));

    let control = tokio::task::spawn_local(pump_loop(
        PumpController::new(SimOutputs(Arc::clone(water)), Arc::clone(&service), TIMING),
        levels.subscribe(),
        service.change_signal(),
        Arc::new(Notify::new()),
        TIMING,
        Arc::new(AtomicU32::new(0)),
    ));

    let addr = start_with_pump_control(Arc::clone(&service)).await;

    (service, control, addr)
}

fn code(response: &str) -> u16 {
    response
        .get(9..12)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status line in: {response}"))
}

impl Sim {
    pub async fn start(layout: Layout) -> Self {
        let water = Arc::new(Mutex::new(Aquarium::new(layout)));
        let levels = Arc::new(InMemoryWaterLevelStore::new());
        let clock = Arc::new(SimClock::new());
        let store = Arc::new(VolatilePumpStore::default());

        spawn_driver(Arc::clone(&water), Arc::clone(&levels));
        let (service, control, addr) = boot(&water, &clock, &store, &levels).await;

        Self {
            water,
            clock,
            store,
            service,
            addr,
            levels,
            control,
        }
    }

    pub fn water(&self) -> MutexGuard<'_, Aquarium> {
        self.water.lock().expect("aquarium poisoned")
    }

    pub async fn restart(&mut self, outage: Duration, synced: bool) {
        self.control.abort();
        self.water().fail_safe();
        tokio::time::sleep(outage).await;
        self.clock.reboot(synced);

        let (service, control, addr) =
            boot(&self.water, &self.clock, &self.store, &self.levels).await;
        self.service = service;
        self.control = control;
        self.addr = addr;
    }

    pub async fn run_for(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
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

            tokio::time::sleep(STEP).await;
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
