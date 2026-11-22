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
        if !on && barrel.refuse_off == Some(channel) {
            return Err("relay driver refused");
        }
        if barrel.valves[channel.index()] != on {
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

#[derive(Default)]
pub struct FakeWaterChange(AtomicBool);

impl WaterChangeActivity for FakeWaterChange {
    fn water_change_active(&self) -> bool {
        self.0.load(Ordering::Relaxed)
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
    pub barrel: Shared,
    pub service: Arc<Service>,
    pub store: Arc<VolatileReservoirStore>,
    pub leak: Arc<FakeLeak>,
    pub water_change: Arc<FakeWaterChange>,
    pub tds_live: Arc<AtomicBool>,
    settings: ReservoirSettings,
    clock: Arc<BarrelClock>,
    levels: Arc<InMemoryWaterLevelStore>,
    telemetry: Arc<InMemoryTelemetryStore>,
    driver: JoinHandle<()>,
    control: JoinHandle<()>,
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
            barrel.step(STEP.as_secs_f64());
            levels.publish(barrel.snapshot());
            if tds_live.load(Ordering::Relaxed) && seconds.is_multiple_of(TDS_PERIOD_S) {
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
        let water_change = Arc::new(FakeWaterChange::default());
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
            water_change,
            tds_live,
            settings,
            clock,
            levels,
            telemetry,
            driver,
            control: tokio::task::spawn_local(async {}),
        };
        sim.boot();
        sim.run_for(Duration::from_secs(3)).await;
        sim
    }

    fn boot(&mut self) {
        self.service = Arc::new(ReservoirService::new(
            self.settings,
            Arc::clone(&self.store),
            Arc::clone(&self.clock),
            None,
            TIMING,
        ));
        self.service.set_supervision(Supervision::Active);
        self.control = tokio::task::spawn_local(reservoir_loop(
            ReservoirController::new(
                BarrelOutputs(Arc::clone(&self.barrel)),
                Arc::clone(&self.service),
                TIMING,
            ),
            self.levels.subscribe(),
            self.telemetry.subscribe_tds(),
            Arc::clone(&self.leak),
            Arc::clone(&self.water_change),
            self.service.change_signal(),
            Arc::new(Notify::new()),
            TIMING,
            Arc::new(AtomicU32::new(0)),
        ));
    }

    pub async fn reboot(&mut self) {
        self.control.abort();
        self.barrel().valves = [false; 4];
        self.boot();
        self.run_for(Duration::from_secs(3)).await;
    }

    pub fn barrel(&self) -> MutexGuard<'_, Barrel> {
        self.barrel.lock().expect("barrel poisoned")
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
        self.water_change.0.store(true, Ordering::Relaxed);
        self.run_for(Duration::from_secs(2)).await;
        self.barrel().water_change_active = true;
    }

    pub fn end_water_change(&self) {
        self.barrel().water_change_active = false;
        self.water_change.0.store(false, Ordering::Relaxed);
    }

    pub fn set_leak(&self, leak: Option<bool>) {
        *self.leak.0.lock().expect("leak poisoned") = leak;
    }
}

impl Drop for BarrelSim {
    fn drop(&mut self) {
        self.control.abort();
        self.driver.abort();
    }
}
