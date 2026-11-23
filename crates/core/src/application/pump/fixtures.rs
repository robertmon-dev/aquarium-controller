use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::sync::mpsc;
use tokio::time::Instant;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::{PumpController, PumpService, PumpTiming};
use crate::domain::cron::CivilTime;
use crate::domain::pump::{
    Checkpoint, LevelConditions, Operation, OperationEvent, OperationPlan, PumpId, PumpSettings,
    PumpSettingsPatch, TopOffWindow, WaterChangePlan,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::{Clock, PumpOutputs, PumpRecord, PumpStore};

pub(super) const EPOCH: u64 = 1_700_000_000;

pub(super) const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    reservoir_ready: true,
    tank_overflow: false,
    sump_full: false,
    sump_high: false,
};

pub(super) const PLAN: WaterChangePlan = WaterChangePlan {
    drain_s: 3,
    settle_s: 2,
    refill_s: 4,
};

#[derive(Default)]
pub(super) struct TestClock {
    epoch: Mutex<Option<u64>>,
    uptime: Mutex<u64>,
    civil: Mutex<Option<CivilTime>>,
}

impl TestClock {
    pub(super) fn synced(epoch: u64) -> Arc<Self> {
        let clock = Self::default();
        *clock.epoch.lock().unwrap() = Some(epoch);
        Arc::new(clock)
    }

    pub(super) fn unsynced() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub(super) fn set_epoch(&self, epoch: Option<u64>) {
        *self.epoch.lock().unwrap() = epoch;
    }

    pub(super) fn set_civil(&self, civil: Option<CivilTime>) {
        *self.civil.lock().unwrap() = civil;
    }

    pub(super) fn advance(&self, seconds: u64) {
        if let Some(epoch) = self.epoch.lock().unwrap().as_mut() {
            *epoch += seconds;
        }
        *self.uptime.lock().unwrap() += seconds;
    }
}

impl Clock for TestClock {
    fn epoch_seconds(&self) -> Option<u64> {
        *self.epoch.lock().unwrap()
    }

    fn uptime_seconds(&self) -> u64 {
        *self.uptime.lock().unwrap()
    }

    fn local_minute_of_day(&self) -> Option<u16> {
        None
    }

    fn local_month(&self) -> Option<u8> {
        None
    }

    fn local_civil_time(&self) -> Option<CivilTime> {
        *self.civil.lock().unwrap()
    }
}

#[derive(Clone, Default)]
pub(super) struct RecordingOutputs {
    pins: Arc<Mutex<[bool; 4]>>,
    pub(super) failing: Arc<Mutex<Option<PumpId>>>,
}

impl RecordingOutputs {
    pub(super) fn state(&self) -> [bool; 4] {
        *self.pins.lock().unwrap()
    }
}

impl PumpOutputs for RecordingOutputs {
    fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
        if *self.failing.lock().unwrap() == Some(id) {
            return Err("stuck relay");
        }

        self.pins.lock().unwrap()[id.index()] = on;
        Ok(())
    }
}

pub(super) type Service = PumpService<Arc<VolatilePumpStore>, Arc<TestClock>>;

pub(super) fn timing() -> PumpTiming {
    PumpTiming::DEFAULT
}

pub(super) fn service_on(store: &Arc<VolatilePumpStore>, clock: Arc<TestClock>) -> Arc<Service> {
    Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        Arc::clone(store),
        clock,
        None,
    ))
}

pub(super) fn service_and_store() -> (Arc<Service>, Arc<VolatilePumpStore>) {
    let store = Arc::new(VolatilePumpStore::default());
    (service_on(&store, TestClock::synced(EPOCH)), store)
}

pub(super) fn decode<T: DeserializeOwned>(
    store: &VolatilePumpStore,
    record: PumpRecord,
) -> Option<T> {
    store
        .get(record)
        .map(|bytes| serde_json::from_slice(&bytes).expect("stored JSON"))
}

pub(super) fn observe(service: &Service, levels: Option<LevelConditions>) {
    service.observe(levels, Duration::ZERO, Instant::now());
}

pub(super) fn snapshot(wet: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = WaterLevelSnapshot::dry();

    for reading in &mut snapshot.sensors {
        if wet.contains(&reading.role) {
            reading.state = WaterLevelState::Wet;
        }
    }

    snapshot
}

pub(super) fn healthy() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
    ])
}

pub(super) fn seed_checkpoint(
    store: &VolatilePumpStore,
    operation: Operation,
    checkpoint_at: Option<u64>,
) {
    store.put(
        PumpRecord::Checkpoint,
        serde_json::to_vec(&Checkpoint {
            operation,
            checkpoint_at,
        })
        .expect("encode"),
    );
}

pub(super) fn interrupted_drain(started_at: u64) -> Operation {
    let mut operation = Operation::start(
        OperationPlan::water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 10,
        }),
        Some(started_at),
    );
    operation.advance(4000, 4000, Some(HEALTHY));
    operation
}

pub(super) struct Rig {
    pub(super) service: Arc<Service>,
    controller: PumpController<RecordingOutputs, Arc<VolatilePumpStore>, Arc<TestClock>>,
    pub(super) outputs: RecordingOutputs,
    pub(super) store: Arc<VolatilePumpStore>,
    pub(super) clock: Arc<TestClock>,
    pub(super) now: Instant,
}

impl Rig {
    pub(super) fn new() -> Self {
        Self::build(
            Arc::new(VolatilePumpStore::default()),
            TestClock::synced(EPOCH),
            None,
        )
    }

    pub(super) fn build(
        store: Arc<VolatilePumpStore>,
        clock: Arc<TestClock>,
        events: Option<mpsc::UnboundedSender<OperationEvent>>,
    ) -> Self {
        let service = Arc::new(PumpService::new(
            PumpSettings::DEFAULT,
            Arc::clone(&store),
            Arc::clone(&clock),
            events,
        ));
        let outputs = RecordingOutputs::default();
        let controller = PumpController::new(outputs.clone(), Arc::clone(&service), timing());

        Self {
            service,
            controller,
            outputs,
            store,
            clock,
            now: Instant::now(),
        }
    }

    pub(super) fn tick(&mut self, levels: Option<&WaterLevelSnapshot>) -> bool {
        self.controller.tick(levels, self.now)
    }

    pub(super) fn run(&mut self, seconds: u64, levels: Option<&WaterLevelSnapshot>) {
        for _ in 0..seconds {
            self.now += Duration::from_secs(1);
            self.clock.advance(1);
            self.tick(levels);
        }
    }

    pub(super) fn pins(&self) -> [bool; 4] {
        self.outputs.state()
    }

    pub(super) fn set_override(&mut self, id: PumpId, duration_s: Option<u32>) {
        self.service.set_override(id, duration_s).expect("valid");
        self.now = self.now.max(Instant::now());
    }
}

pub(super) fn sunday_ten_am() -> CivilTime {
    CivilTime {
        minute: 0,
        hour: 10,
        day: 7,
        month: 6,
        weekday: 0,
    }
}

pub(super) fn schedule_top_off_on<S: PumpStore, C: Clock>(service: &PumpService<S, C>) {
    service
        .apply_settings(PumpSettingsPatch {
            top_off: Some(vec![TopOffWindow {
                cron: "0 10 * * sun".into(),
                duration_s: 120,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

pub(super) fn set_refill_stop(service: &Service, role: WaterLevelRole) {
    service
        .apply_settings(PumpSettingsPatch {
            refill_stop_sensor: Some(Some(role)),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

pub(super) fn full_sump() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpReturnLevel,
    ])
}

pub(super) fn reservoir_patch(drain_lph: u32, reservoir_l: Option<u32>) -> PumpSettingsPatch {
    PumpSettingsPatch {
        flow_lph: Some(crate::domain::pump::FlowRates {
            makeup: Some(600),
            drain: Some(drain_lph),
        }),
        reservoir_ready_l: Some(reservoir_l),
        ..PumpSettingsPatch::default()
    }
}

pub(super) fn low_sump() -> WaterLevelSnapshot {
    snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady])
}

pub(super) fn starved(rig: &Rig) -> bool {
    rig.service.status().circulation_starved
}

pub(super) fn running_sump() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReturnLevel,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
    ])
}
