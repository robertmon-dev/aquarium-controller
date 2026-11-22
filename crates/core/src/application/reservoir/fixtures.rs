use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::time::Instant;

use super::{ReservoirController, ReservoirService, ReservoirTiming};
use crate::adapters::outbound::memory::VolatileReservoirStore;
use crate::domain::cron::CivilTime;
use crate::domain::pump::Supervision;
use crate::domain::reservoir::{
    OverrideTarget, ReservoirChannel, ReservoirSettings, ReservoirStatus,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::inbound::{FillCancelError, FillStartError, ReservoirOverrideError};
use crate::ports::outbound::{Clock, ReservoirOutputs, ReservoirRecord, ReservoirStore, TdsSample};

pub(super) const EPOCH: u64 = 1_800_000_000;

#[derive(Default)]
pub(super) struct TestClock {
    epoch: AtomicU64,
}

impl TestClock {
    pub(super) fn synced() -> Arc<Self> {
        Arc::new(Self {
            epoch: AtomicU64::new(EPOCH),
        })
    }

    pub(super) fn advance(&self, seconds: u64) {
        self.epoch.fetch_add(seconds, Ordering::Relaxed);
    }
}

impl Clock for TestClock {
    fn epoch_seconds(&self) -> Option<u64> {
        Some(self.epoch.load(Ordering::Relaxed))
    }

    fn uptime_seconds(&self) -> u64 {
        0
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

#[derive(Clone, Default)]
pub(super) struct Relays {
    pins: Arc<Mutex<[bool; 4]>>,
    refuse_off: Arc<Mutex<Option<ReservoirChannel>>>,
    refuse_on: Arc<Mutex<Option<ReservoirChannel>>>,
}

impl Relays {
    pub(super) fn pin(&self, channel: ReservoirChannel) -> bool {
        self.pins.lock().unwrap()[channel.index()]
    }

    pub(super) fn pins(&self) -> [bool; 4] {
        *self.pins.lock().unwrap()
    }

    pub(super) fn refuse_off(&self, channel: Option<ReservoirChannel>) {
        *self.refuse_off.lock().unwrap() = channel;
    }

    pub(super) fn refuse_on(&self, channel: Option<ReservoirChannel>) {
        *self.refuse_on.lock().unwrap() = channel;
    }

    fn stuck(&self) -> Option<ReservoirChannel> {
        *self.refuse_off.lock().unwrap()
    }
}

impl ReservoirOutputs for Relays {
    fn set(&mut self, channel: ReservoirChannel, on: bool) -> Result<(), &'static str> {
        let refused = if on {
            &self.refuse_on
        } else {
            &self.refuse_off
        };
        if *refused.lock().unwrap() == Some(channel) {
            return Err("relay driver refused");
        }
        self.pins.lock().unwrap()[channel.index()] = on;
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct FlakyStore {
    pub(super) inner: VolatileReservoirStore,
    pub(super) refuse_marker: AtomicBool,
    pub(super) refuse_fault: AtomicBool,
    pub(super) refuse_erase: AtomicBool,
    pub(super) marker_writes: AtomicU32,
}

impl FlakyStore {
    pub(super) fn corrupt(&self, record: ReservoirRecord) {
        self.inner
            .store(record, b"{not json")
            .expect("volatile store accepts");
    }

    pub(super) fn decode<T: DeserializeOwned>(&self, record: ReservoirRecord) -> Option<T> {
        self.inner
            .get(record)
            .map(|bytes| serde_json::from_slice(&bytes).expect("readable record"))
    }
}

impl ReservoirStore for FlakyStore {
    fn load(&self, record: ReservoirRecord) -> Option<Vec<u8>> {
        self.inner.load(record)
    }

    fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str> {
        let refused = match record {
            ReservoirRecord::Running => {
                self.marker_writes.fetch_add(1, Ordering::Relaxed);
                self.refuse_marker.load(Ordering::Relaxed)
            }
            ReservoirRecord::Fault => self.refuse_fault.load(Ordering::Relaxed),
            _ => false,
        };
        if refused {
            return Err("flash refused the write");
        }
        self.inner.store(record, bytes)
    }

    fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str> {
        if record == ReservoirRecord::Running && self.refuse_erase.load(Ordering::Relaxed) {
            return Err("flash refused the erase");
        }
        self.inner.erase(record)
    }
}

pub(super) const HALF: [WaterLevelRole; 2] =
    [WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady];
pub(super) const FULL: [WaterLevelRole; 3] = [
    WaterLevelRole::SumpReserve,
    WaterLevelRole::ReservoirReady,
    WaterLevelRole::ReservoirFull,
];

pub(super) fn barrel(wet: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = WaterLevelSnapshot::dry();
    for role in wet {
        snapshot.sensors[role.index()].state = WaterLevelState::Wet;
    }
    snapshot
}

pub(super) type Service = ReservoirService<Arc<FlakyStore>, Arc<TestClock>>;

pub(super) struct Rig {
    pub(super) service: Arc<Service>,
    pub(super) controller: ReservoirController<Relays, Arc<FlakyStore>, Arc<TestClock>>,
    pub(super) relays: Relays,
    pub(super) store: Arc<FlakyStore>,
    pub(super) clock: Arc<TestClock>,
    pub(super) settings: ReservoirSettings,
    pub(super) now: Instant,
    pub(super) levels: WaterLevelSnapshot,
    pub(super) levels_fresh: bool,
    pub(super) tds: Option<f32>,
    pub(super) leak: Option<bool>,
    pub(super) water_change: bool,
}

impl Rig {
    pub(super) fn new(settings: ReservoirSettings) -> Self {
        Self::build(Arc::default(), TestClock::synced(), settings)
    }

    pub(super) fn build(
        store: Arc<FlakyStore>,
        clock: Arc<TestClock>,
        settings: ReservoirSettings,
    ) -> Self {
        let service = Arc::new(ReservoirService::new(
            settings,
            Arc::clone(&store),
            Arc::clone(&clock),
            None,
            ReservoirTiming::DEFAULT,
        ));
        service.set_supervision(Supervision::Active);
        let relays = Relays::default();
        let controller = ReservoirController::new(
            relays.clone(),
            Arc::clone(&service),
            ReservoirTiming::DEFAULT,
        );
        let mut rig = Self {
            service,
            controller,
            relays,
            store,
            clock,
            settings,
            now: Instant::now(),
            levels: barrel(&HALF),
            levels_fresh: true,
            tds: None,
            leak: None,
            water_change: false,
        };
        rig.tick();
        rig
    }

    pub(super) fn reboot(&self) -> Self {
        Self::build(
            Arc::clone(&self.store),
            Arc::clone(&self.clock),
            self.settings,
        )
    }

    pub(super) fn tick(&mut self) -> bool {
        let tds = self.tds.map(|ppm| TdsSample {
            ppm: Some(ppm),
            measured_at: self.now,
        });
        let levels = self.levels_fresh.then_some(&self.levels);
        let changed = self
            .controller
            .tick(levels, tds, self.leak, self.water_change, self.now);
        self.check_invariants();
        changed
    }

    fn check_invariants(&self) {
        use ReservoirChannel::{Booster, RoValve, TapValve};

        let pins = self.relays.pins();
        let stuck = self.relays.stuck();
        let on = |channel: ReservoirChannel| pins[channel.index()] && stuck != Some(channel);
        assert!(
            !(pins[RoValve.index()] && pins[TapValve.index()]),
            "both inlet valves are open"
        );
        assert!(
            !on(Booster) || pins[RoValve.index()],
            "the booster runs with the RO valve closed"
        );

        let status = self.status();
        let blocked = status.fault.is_some()
            || status.supervision != Supervision::Active
            || self.leak == Some(true)
            || !self.levels_fresh;
        if blocked {
            for channel in ReservoirChannel::ALL {
                assert!(!on(channel), "{channel:?} is on under a global blocker");
            }
        }
    }

    pub(super) fn run(&mut self, seconds: u64) {
        for _ in 0..seconds {
            self.now += Duration::from_secs(1);
            self.clock.advance(1);
            self.tick();
        }
    }

    pub(super) fn start_fill(&self) -> Result<(), FillStartError> {
        self.service.start_fill_at(self.now)
    }

    pub(super) fn cancel_fill(&self) -> Result<(), FillCancelError> {
        self.service.cancel_fill_at(self.now)
    }

    pub(super) fn set_override(
        &self,
        target: OverrideTarget,
        on: bool,
        duration_s: Option<u32>,
    ) -> Result<ReservoirStatus, ReservoirOverrideError> {
        self.service
            .set_override_at(target, on, duration_s, self.now)
    }

    pub(super) fn status(&self) -> ReservoirStatus {
        self.service.status_at(self.now)
    }
}

pub(super) const RO_ONLY: ReservoirSettings = ReservoirSettings::DEFAULT;
pub(super) const TARGETED: ReservoirSettings = ReservoirSettings {
    target_tds_ppm: Some(150),
    ..ReservoirSettings::DEFAULT
};
