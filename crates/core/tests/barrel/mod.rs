mod model;
pub use model::{Barrel, BarrelLayout};

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use aquarium_core::adapters::outbound::memory::{
    InMemoryTelemetryStore, InMemoryWaterLevelStore, VolatileReservoirStore,
};
use aquarium_core::application::{
    reservoir_loop, ReservoirController, ReservoirService, ReservoirTiming,
};
use aquarium_core::domain::cron::CivilTime;
use aquarium_core::domain::pump::Supervision;
use aquarium_core::domain::reservoir::{
    FillRecord, ReservoirChannel, ReservoirSettings, ReservoirStatus,
};

use aquarium_core::ports::inbound::ReservoirControl;
use aquarium_core::ports::outbound::{
    Clock, LeakSource, ReservoirOutputs, TdsSample, TdsSampleSink, WaterChangeActivity,
    WaterLevelSink,
};

pub const STEP: Duration = Duration::from_secs(1);
pub const TDS_PERIOD_S: u64 = 2;
pub const BASE_EPOCH: u64 = 1_800_000_000;
pub const TIMING: ReservoirTiming = ReservoirTiming {
    tick: Duration::from_secs(1),
    level_stale: Duration::from_secs(3),
    tds_stale: Duration::from_secs(5),
    runtime_flush: Duration::from_secs(600),
};

type Shared = Arc<Mutex<Barrel>>;

struct BarrelOutputs(Shared);

impl ReservoirOutputs for BarrelOutputs {
    fn set(&mut self, channel: ReservoirChannel, on: bool) -> Result<(), &'static str> {
        let mut barrel = self.0.lock().expect("barrel poisoned");
        if barrel.valves[channel.index()] != on {
            barrel.advance();
            barrel.valves[channel.index()] = on;
            let at = barrel.t_s;
            barrel.runs.push((channel, on, at));
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct FakeLeak(Mutex<Option<bool>>);

impl LeakSource for FakeLeak {
    fn leak(&self) -> Option<bool> {
        *self.0.lock().expect("leak poisoned")
    }
}

struct FakeWaterChange(Shared);

impl WaterChangeActivity for FakeWaterChange {
    fn water_change_active(&self) -> bool {
        self.0.lock().expect("barrel poisoned").water_change_active
    }
}

pub struct BarrelClock {
    start: Instant,
}

impl Clock for BarrelClock {
    fn epoch_seconds(&self) -> Option<u64> {
        Some(BASE_EPOCH + self.start.elapsed().as_secs())
    }
    fn uptime_seconds(&self) -> u64 {
        self.start.elapsed().as_secs()
    }
    fn local_minute_of_day(&self) -> Option<u16> {
        None
    }
    fn local_month(&self) -> Option<u8> {
        None
    }
    fn local_civil_time(&self) -> Option<CivilTime> {
        None
    }
}

pub type Service = ReservoirService<Arc<VolatileReservoirStore>, Arc<BarrelClock>>;

pub struct BarrelSim {
    barrel: Shared,
    pub service: Arc<Service>,
    store: Arc<VolatileReservoirStore>,
    leak: Arc<FakeLeak>,
    pub tds_live: Arc<AtomicBool>,
    settings: ReservoirSettings,
    clock: Arc<BarrelClock>,
    levels: Arc<InMemoryWaterLevelStore>,
    telemetry: Arc<InMemoryTelemetryStore>,
    driver: JoinHandle<()>,
    control: Option<JoinHandle<()>>,
}

async fn drive(
    barrel: Shared,
    levels: Arc<InMemoryWaterLevelStore>,
    telemetry: Arc<InMemoryTelemetryStore>,
    tds_live: Arc<AtomicBool>,
) {
    let mut ticker = tokio::time::interval(STEP);
    let mut seconds = 0_u64;
    loop {
        ticker.tick().await;
        {
            let mut barrel = barrel.lock().expect("barrel poisoned");
            barrel.advance();
            levels.publish(barrel.snapshot());
            if tds_live.load(Ordering::Relaxed) && seconds % TDS_PERIOD_S == 0 {
                telemetry.publish_tds(TdsSample {
                    ppm: Some(barrel.probe_ppm as f32),
                    measured_at: Instant::now(),
                });
            }
        }
        seconds += 1;
    }
}

impl BarrelSim {
    pub async fn start(barrel: Barrel, settings: ReservoirSettings) -> Self {
        let barrel = Arc::new(Mutex::new(barrel));
        let levels = Arc::new(InMemoryWaterLevelStore::new());
        let telemetry = Arc::new(InMemoryTelemetryStore::new());
        let tds_live = Arc::new(AtomicBool::new(true));
        let leak = Arc::new(FakeLeak::default());
        let store = Arc::new(VolatileReservoirStore::default());
        let clock = Arc::new(BarrelClock {
            start: Instant::now(),
        });

        let driver = tokio::task::spawn_local(drive(
            Arc::clone(&barrel),
            Arc::clone(&levels),
            Arc::clone(&telemetry),
            Arc::clone(&tds_live),
        ));

        let mut sim = Self {
            service: Arc::new(ReservoirService::new(
                settings,
                Arc::clone(&store),
                Arc::clone(&clock),
                None,
                TIMING,
            )),
            barrel,
            store,
            leak,
            tds_live,
            settings,
            clock,
            levels,
            telemetry,
            driver,
            control: None,
        };
        sim.start_controller();
        sim.run_for(Duration::from_secs(3)).await;
        sim
    }

    fn start_controller(&mut self) {
        self.service.set_supervision(Supervision::Active);
        self.control = Some(tokio::task::spawn_local(reservoir_loop(
            ReservoirController::new(
                BarrelOutputs(Arc::clone(&self.barrel)),
                Arc::clone(&self.service),
                TIMING,
            ),
            self.levels.subscribe(),
            self.telemetry.subscribe_tds(),
            Arc::clone(&self.leak),
            FakeWaterChange(Arc::clone(&self.barrel)),
            self.service.change_signal(),
            Arc::new(Notify::new()),
            TIMING,
            Arc::new(AtomicU32::new(0)),
        )));
    }

    pub async fn reboot(&mut self) {
        if let Some(task) = self.control.take() {
            task.abort();
        }
        self.barrel().valves = [false; 4];
        self.service = Arc::new(ReservoirService::new(
            self.settings,
            Arc::clone(&self.store),
            Arc::clone(&self.clock),
            None,
            TIMING,
        ));
        self.start_controller();
        self.run_for(Duration::from_secs(3)).await;
    }

    pub fn barrel(&self) -> MutexGuard<'_, Barrel> {
        let mut barrel = self.barrel.lock().expect("barrel poisoned");
        barrel.advance();
        barrel
    }

    pub fn status(&self) -> ReservoirStatus {
        self.service.status()
    }

    pub async fn run_for(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }

    pub async fn run_until(
        &self,
        limit: Duration,
        what: &str,
        mut done: impl FnMut(&ReservoirStatus, &Barrel) -> bool,
    ) {
        let deadline = Instant::now() + limit;
        loop {
            {
                let status = self.status();
                let barrel = self.barrel();
                if done(&status, &barrel) {
                    return;
                }
                assert!(
                    Instant::now() < deadline,
                    "timed out waiting for {what}: {status:?}"
                );
            }
            tokio::time::sleep(STEP).await;
        }
    }

    pub async fn finished(&self, limit: Duration) -> FillRecord {
        self.run_until(limit, "the fill to finish", |status, _| {
            status.fill.is_none() && status.last.is_some()
        })
        .await;
        self.status().last.expect("a fill record")
    }

    pub async fn fills_during(&self, duration: Duration) -> Vec<FillRecord> {
        let deadline = Instant::now() + duration;
        let mut seen = self.status().last;
        let mut records = Vec::new();
        while Instant::now() < deadline {
            tokio::time::sleep(STEP).await;
            let last = self.status().last;
            if last != seen {
                records.extend(last);
                seen = last;
            }
        }
        records
    }

    pub async fn begin_water_change(&self) {
        self.barrel().water_change_active = true;
        self.levels.publish(self.barrel().snapshot());
        tokio::task::yield_now().await;
    }

    pub fn end_water_change(&self) {
        self.barrel().water_change_active = false;
    }

    pub fn set_leak(&self, leak: Option<bool>) {
        *self.leak.0.lock().expect("leak poisoned") = leak;
    }
}

impl Drop for BarrelSim {
    fn drop(&mut self) {
        if let Some(task) = self.control.take() {
            task.abort();
        }
        self.driver.abort();
    }
}
