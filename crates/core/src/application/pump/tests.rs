use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::sync::{mpsc, Notify};
use tokio::task::LocalSet;
use tokio::time::Instant;

use super::*;
use crate::adapters::outbound::memory::{InMemoryWaterLevelStore, VolatilePumpStore};
use crate::domain::cron::CivilTime;
use crate::domain::pump::{
    Blocker, ChannelFault, ChannelMode, Checkpoint, LastOperations, LevelConditions, Operation,
    OperationEvent, OperationKind, OperationPlan, OperationResult, OperationState, OperationStatus,
    OverrideError, Phase, PumpId, PumpSettings, PumpSettingsPatch, RuntimeCounters, SkipReason,
    SkippedWindow, StoredOverride, Supervision, TopOffWindow, WaterChangePlan,
};
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};
use crate::ports::inbound::{PumpControl, PumpSettingsError, StartError};
use crate::ports::outbound::{Clock, PumpOutputs, PumpRecord, PumpStore, WaterLevelSink};

const EPOCH: u64 = 1_700_000_000;

const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    reservoir_ready: true,
    tank_overflow: false,
    sump_full: false,
    sump_high: false,
};

const PLAN: WaterChangePlan = WaterChangePlan {
    drain_s: 3,
    settle_s: 2,
    refill_s: 4,
};

#[derive(Default)]
struct TestClock {
    epoch: Mutex<Option<u64>>,
    uptime: Mutex<u64>,
    civil: Mutex<Option<CivilTime>>,
}

impl TestClock {
    fn synced(epoch: u64) -> Arc<Self> {
        let clock = Self::default();
        *clock.epoch.lock().unwrap() = Some(epoch);
        Arc::new(clock)
    }

    fn unsynced() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn set_epoch(&self, epoch: Option<u64>) {
        *self.epoch.lock().unwrap() = epoch;
    }

    fn set_civil(&self, civil: Option<CivilTime>) {
        *self.civil.lock().unwrap() = civil;
    }

    fn advance(&self, seconds: u64) {
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
struct RecordingOutputs {
    pins: Arc<Mutex<[bool; 4]>>,
    failing: Arc<Mutex<Option<PumpId>>>,
}

impl RecordingOutputs {
    fn state(&self) -> [bool; 4] {
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

struct FailingStore;

impl PumpStore for FailingStore {
    fn load(&self, _record: PumpRecord) -> Option<Vec<u8>> {
        None
    }

    fn store(&self, _record: PumpRecord, _bytes: &[u8]) -> Result<(), &'static str> {
        Err("flash is full")
    }

    fn erase(&self, _record: PumpRecord) -> Result<(), &'static str> {
        Ok(())
    }
}

type Service = PumpService<Arc<VolatilePumpStore>, Arc<TestClock>>;

fn timing() -> PumpTiming {
    PumpTiming::DEFAULT
}

fn service_on(store: &Arc<VolatilePumpStore>, clock: Arc<TestClock>) -> Arc<Service> {
    Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        Arc::clone(store),
        clock,
        None,
    ))
}

fn service_and_store() -> (Arc<Service>, Arc<VolatilePumpStore>) {
    let store = Arc::new(VolatilePumpStore::default());
    (service_on(&store, TestClock::synced(EPOCH)), store)
}

fn decode<T: DeserializeOwned>(store: &VolatilePumpStore, record: PumpRecord) -> Option<T> {
    store
        .get(record)
        .map(|bytes| serde_json::from_slice(&bytes).expect("stored JSON"))
}

fn observe(service: &Service, levels: Option<LevelConditions>) {
    service.observe(levels, Duration::ZERO, Instant::now());
}

#[test]
fn a_fresh_service_reports_unavailable_channels_and_no_operation() {
    let (service, _store) = service_and_store();
    let status = service.status();

    for id in PumpId::ALL {
        assert_eq!(
            status.channels.get(id).fault,
            Some(ChannelFault::Unavailable)
        );
    }

    assert_eq!(status.operation, OperationStatus::IDLE);
    assert_eq!(service.settings(), PumpSettings::DEFAULT);
}

#[test]
fn settings_are_validated_persisted_and_reloaded() {
    let (service, store) = service_and_store();
    let patch = PumpSettingsPatch {
        top_off: Some(vec![TopOffWindow {
            cron: "0 8 * * *".into(),
            duration_s: 60,
        }]),
        ..PumpSettingsPatch::default()
    };

    let applied = service.apply_settings(patch).expect("valid");
    assert_eq!(
        decode::<PumpSettings>(&store, PumpRecord::Settings),
        Some(applied.clone())
    );

    let invalid = PumpSettingsPatch {
        resume_window_s: Some(9999),
        ..PumpSettingsPatch::default()
    };
    assert!(matches!(
        service.apply_settings(invalid),
        Err(PumpSettingsError::Invalid(_))
    ));
    assert_eq!(service.settings(), applied);

    assert_eq!(
        service_on(&store, TestClock::synced(EPOCH)).settings(),
        applied
    );
}

#[test]
fn a_settings_write_failure_is_reported() {
    let service = PumpService::new(
        PumpSettings::DEFAULT,
        FailingStore,
        TestClock::synced(EPOCH),
        None,
    );
    let patch = PumpSettingsPatch {
        resume_window_s: Some(60),
        ..PumpSettingsPatch::default()
    };

    assert_eq!(
        service.apply_settings(patch),
        Err(PumpSettingsError::NotPersisted("flash is full"))
    );
    assert_eq!(service.settings(), PumpSettings::DEFAULT);
}

#[test]
fn unreadable_records_fall_back_to_defaults() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(PumpRecord::Settings, b"not json".to_vec());
    store.put(PumpRecord::Checkpoint, b"{".to_vec());

    let service = service_on(&store, TestClock::synced(EPOCH));

    assert_eq!(service.settings(), PumpSettings::DEFAULT);
    assert_eq!(service.status().operation, OperationStatus::IDLE);
}

#[test]
fn overrides_are_validated_and_persisted() {
    let (service, store) = service_and_store();

    assert_eq!(
        service.set_override(PumpId::Return, None),
        Err(OverrideError::Invalid(
            "a return pump override needs duration_s"
        ))
    );
    assert_eq!(
        service.set_override(PumpId::Makeup, Some(600)),
        Err(OverrideError::NotOverridable)
    );
    assert_eq!(
        service.clear_override(PumpId::Drain),
        Err(OverrideError::NotOverridable)
    );

    let status = service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");
    assert_eq!(status.mode, ChannelMode::ManualOff);
    assert_eq!(status.override_expires_in_s, Some(600));
    service.set_override(PumpId::Uv, None).expect("valid");

    assert_eq!(
        decode::<[Option<StoredOverride>; 2]>(&store, PumpRecord::Overrides),
        Some([
            Some(StoredOverride {
                expires_at: Some(EPOCH + 600)
            }),
            Some(StoredOverride { expires_at: None }),
        ])
    );

    service.clear_override(PumpId::Return).expect("overridable");
    assert_eq!(
        decode::<[Option<StoredOverride>; 2]>(&store, PumpRecord::Overrides),
        Some([None, Some(StoredOverride { expires_at: None })])
    );
}

#[test]
fn operations_need_levels_and_respect_blockers() {
    let (service, _store) = service_and_store();

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::LevelsUnavailable))
    );
    assert_eq!(
        service.start_top_off(0),
        Err(StartError::Invalid("a top-off lasts 1 to 600 s"))
    );
    assert_eq!(
        service.start_water_change(WaterChangePlan { drain_s: 0, ..PLAN }),
        Err(StartError::Invalid("drain_s must be between 1 and 900"))
    );

    observe(
        &service,
        Some(LevelConditions {
            reservoir_empty: true,
            ..HEALTHY
        }),
    );
    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::ReservoirEmpty))
    );

    observe(
        &service,
        Some(LevelConditions {
            tank_overflow: true,
            ..HEALTHY
        }),
    );
    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::TankOverflow))
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::TankOverflow))
    );

    observe(&service, Some(HEALTHY));
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_high_sump_refuses_both_operations() {
    let (service, _store) = service_and_store();
    observe(
        &service,
        Some(LevelConditions {
            sump_high: true,
            ..HEALTHY
        }),
    );

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::SumpHigh))
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::SumpHigh))
    );
}

#[test]
fn a_flooded_sump_ends_a_running_top_off_with_an_overflow() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let flooded = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpMechanical,
    ]);

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(2, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.run(1, Some(&flooded));
    assert!(!rig.pins()[2]);
    let status = rig.service.status();
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpHigh));
    assert!(status.channels.return_pump.on);
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::Overflow)
    );
}

#[test]
fn a_water_change_needs_a_ready_reservoir_but_a_top_off_does_not() {
    let (service, _store) = service_and_store();
    observe(
        &service,
        Some(LevelConditions {
            reservoir_ready: false,
            ..HEALTHY
        }),
    );

    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::ReservoirLow))
    );
    assert_eq!(service.start_top_off(60), Ok(()));
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::ReservoirLow))
    );
    assert_eq!(
        service.status().operation.kind,
        Some(OperationKind::TopOff),
        "a refused water change must not abort the running top-off"
    );
    service.cancel(OperationKind::TopOff).expect("cancelled");

    observe(&service, Some(HEALTHY));
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_water_change_preempts_a_top_off_but_not_the_other_way_round() {
    let (service, store) = service_and_store();
    observe(&service, Some(HEALTHY));

    service.start_top_off(60).expect("idle");
    assert_eq!(service.start_top_off(60), Err(StartError::Conflict));

    service.start_water_change(PLAN).expect("preempts");
    let status = service.status();
    assert_eq!(status.operation.kind, Some(OperationKind::WaterChange));
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::Aborted)
    );

    assert_eq!(service.start_top_off(60), Err(StartError::Conflict));
    assert_eq!(service.start_water_change(PLAN), Err(StartError::Conflict));

    let checkpoint: Checkpoint = decode(&store, PumpRecord::Checkpoint).expect("checkpoint");
    assert_eq!(checkpoint.operation.plan.kind, OperationKind::WaterChange);
    assert_eq!(checkpoint.checkpoint_at, Some(EPOCH));
}

#[test]
fn cancelling_records_an_abort_and_erases_the_checkpoint() {
    let (service, store) = service_and_store();
    observe(&service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");

    service.cancel(OperationKind::TopOff).expect("cancelled");
    assert_eq!(
        service.status().operation.kind,
        Some(OperationKind::WaterChange)
    );

    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    let status = service.status();
    assert_eq!(status.operation, OperationStatus::IDLE);
    assert_eq!(
        status.last_water_change.map(|record| record.result),
        Some(OperationResult::Aborted)
    );
    assert_eq!(store.get(PumpRecord::Checkpoint), None);
    assert!(store.get(PumpRecord::LastOperations).is_some());
}

#[test]
fn resetting_a_counter_stamps_the_time() {
    let (service, store) = service_and_store();

    service.reset_runtime(PumpId::Uv);

    let uv = service.status().channels.uv;
    assert_eq!(uv.runtime_s, 0);
    assert_eq!(uv.runtime_reset_at, Some(EPOCH));

    let counters: RuntimeCounters = decode(&store, PumpRecord::Runtime).expect("stored");
    assert_eq!(counters.reset_at[PumpId::Uv.index()], Some(EPOCH));
}

fn snapshot(wet: &[WaterLevelRole]) -> WaterLevelSnapshot {
    let mut snapshot = WaterLevelSnapshot::dry();

    for reading in &mut snapshot.sensors {
        if wet.contains(&reading.role) {
            reading.state = WaterLevelState::Wet;
        }
    }

    snapshot
}

fn healthy() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
    ])
}

fn seed_checkpoint(store: &VolatilePumpStore, operation: Operation, checkpoint_at: Option<u64>) {
    store.put(
        PumpRecord::Checkpoint,
        serde_json::to_vec(&Checkpoint {
            operation,
            checkpoint_at,
        })
        .expect("encode"),
    );
}

fn interrupted_drain(started_at: u64) -> Operation {
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

struct Rig {
    service: Arc<Service>,
    controller: PumpController<RecordingOutputs, Arc<VolatilePumpStore>, Arc<TestClock>>,
    outputs: RecordingOutputs,
    store: Arc<VolatilePumpStore>,
    clock: Arc<TestClock>,
    now: Instant,
}

impl Rig {
    fn new() -> Self {
        Self::build(
            Arc::new(VolatilePumpStore::default()),
            TestClock::synced(EPOCH),
            None,
        )
    }

    fn build(
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

    fn tick(&mut self, levels: Option<&WaterLevelSnapshot>) -> bool {
        self.controller.tick(levels, self.now)
    }

    fn run(&mut self, seconds: u64, levels: Option<&WaterLevelSnapshot>) {
        for _ in 0..seconds {
            self.now += Duration::from_secs(1);
            self.clock.advance(1);
            self.tick(levels);
        }
    }

    fn pins(&self) -> [bool; 4] {
        self.outputs.state()
    }

    fn set_override(&mut self, id: PumpId, duration_s: Option<u32>) {
        self.service.set_override(id, duration_s).expect("valid");
        self.now = self.now.max(Instant::now());
    }
}

#[test]
fn healthy_levels_run_circulation_and_report_a_change_once() {
    let mut rig = Rig::new();
    let healthy = healthy();

    assert!(rig.tick(Some(&healthy)));
    assert_eq!(rig.pins(), [true, true, false, false]);

    rig.now += Duration::from_millis(100);
    assert!(!rig.tick(Some(&healthy)));
}

#[test]
fn missing_levels_keep_circulation_and_block_water_movers() {
    let mut rig = Rig::new();

    rig.tick(None);

    assert_eq!(rig.pins(), [true, true, false, false]);
    let status = rig.service.status();
    assert_eq!(
        status.channels.makeup.blocked_by,
        Some(Blocker::LevelsUnavailable)
    );
    assert_eq!(
        status.channels.drain.blocked_by,
        Some(Blocker::LevelsUnavailable)
    );
}

#[test]
fn the_return_pump_restarts_only_after_thirty_wet_seconds() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve]);

    rig.tick(Some(&healthy));
    rig.run(1, Some(&low));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(30, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn a_top_off_runs_makeup_then_reports_the_result() {
    let (events, mut received) = mpsc::unbounded_channel();
    let mut rig = Rig::build(
        Arc::new(VolatilePumpStore::default()),
        TestClock::synced(EPOCH),
        Some(events),
    );
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_top_off(3).expect("idle");
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, true, true, false]);

    rig.run(3, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);

    let status = rig.service.status();
    assert_eq!(status.operation, OperationStatus::IDLE);
    let record = status.last_top_off.expect("recorded");
    assert_eq!(
        (record.result, record.refill_ran_s),
        (OperationResult::Completed, 3)
    );

    let event = received.try_recv().expect("event");
    assert_eq!(event.record, record);
    assert_eq!(event.finished_at, Some(EPOCH + 3));
}

#[test]
fn a_water_change_drains_settles_refills_and_stops_on_overflow() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let overflow = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::MainTank,
    ]);

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 2,
            settle_s: 2,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);
    let status = rig.service.status();
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
    assert_eq!(status.channels.uv.blocked_by, Some(Blocker::ReturnOff));

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        rig.service.status().channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, true, false]);
    assert_eq!(rig.service.status().channels.return_pump.blocked_by, None);

    rig.run(1, Some(&overflow));
    assert_eq!(rig.pins(), [true, true, false, false]);

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::Overflow);
    assert_eq!((record.drain_ran_s, record.settle_ran_s), (2, 2));
}

#[test]
fn losing_the_level_sensors_aborts_a_water_change() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&healthy));
    rig.run(1, None);

    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::Aborted)
    );
}

#[test]
fn a_failed_output_is_reported_as_a_fault_in_its_last_known_state() {
    let mut rig = Rig::new();
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Uv);

    rig.tick(Some(&healthy()));

    let uv = rig.service.status().channels.uv;
    assert_eq!((uv.on, uv.fault), (true, Some(ChannelFault::OutputError)));

    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Makeup);
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy()));

    let makeup = rig.service.status().channels.makeup;
    assert_eq!(
        (makeup.on, makeup.fault),
        (false, Some(ChannelFault::OutputError))
    );
}

#[test]
fn runtime_accumulates_while_on_and_is_flushed_on_changes() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.run(10, Some(&healthy));

    let status = rig.service.status();
    assert_eq!(status.channels.return_pump.runtime_s, 10);
    assert_eq!(status.channels.makeup.runtime_s, 0);

    rig.run(1, Some(&snapshot(&[WaterLevelRole::SumpReserve])));

    let counters: RuntimeCounters = decode(&rig.store, PumpRecord::Runtime).expect("flushed");
    assert_eq!(counters.runtime_s[PumpId::Return.index()], 11);
}

#[test]
fn the_default_timing_checkpoints_every_three_seconds() {
    assert_eq!(
        PumpTiming::DEFAULT.checkpoint_interval,
        Duration::from_secs(3)
    );
    assert_eq!(
        PumpTiming::DEFAULT.return_restart_delay,
        Duration::from_secs(30)
    );
    assert_eq!(PumpTiming::DEFAULT.level_stale, Duration::from_secs(1));
    assert_eq!(PumpTiming::DEFAULT.tick, Duration::from_secs(1));
    assert_eq!(PumpTiming::DEFAULT.runtime_flush, Duration::from_secs(600));
}

#[test]
fn a_running_operation_is_checkpointed_every_three_seconds() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 60,
            settle_s: 0,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));

    rig.run(2, Some(&healthy));
    let checkpoint: Checkpoint = decode(&rig.store, PumpRecord::Checkpoint).expect("saved");
    assert_eq!(checkpoint.operation.ran_ms[Phase::Draining.index()], 0);

    rig.run(1, Some(&healthy));
    let checkpoint: Checkpoint = decode(&rig.store, PumpRecord::Checkpoint).expect("saved");
    assert_eq!(checkpoint.operation.ran_ms[Phase::Draining.index()], 3_000);
    assert_eq!(checkpoint.checkpoint_at, Some(EPOCH + 3));

    rig.run(3, Some(&healthy));
    let checkpoint: Checkpoint = decode(&rig.store, PumpRecord::Checkpoint).expect("saved");
    assert_eq!(checkpoint.operation.ran_ms[Phase::Draining.index()], 6_000);
}

#[test]
fn a_timed_uv_override_expires_with_the_clock() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.set_override(PumpId::Uv, Some(60));
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, false, false, false]);

    rig.run(59, Some(&healthy));
    assert!(!rig.pins()[1]);

    rig.run(1, Some(&healthy));
    assert!(rig.pins()[1]);
    assert_eq!(
        decode::<[Option<StoredOverride>; 2]>(&rig.store, PumpRecord::Overrides),
        Some([None, None])
    );
}

#[test]
fn a_stored_return_override_waits_for_time_sync_while_uv_stays_off() {
    let store = Arc::new(VolatilePumpStore::default());
    let stored = Some(StoredOverride {
        expires_at: Some(EPOCH + 600),
    });
    store.put(
        PumpRecord::Overrides,
        serde_json::to_vec(&[stored, stored]).expect("encode"),
    );
    let mut rig = Rig::build(store, TestClock::unsynced(), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, false, false, false]);

    rig.clock.set_epoch(Some(EPOCH));
    rig.run(1, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn an_interrupted_water_change_resumes_after_a_short_outage() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 200), Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);
    let operation = rig.service.status().operation;
    assert_eq!(operation.state, OperationState::Draining);
    assert_eq!(operation.phase_remaining_s, Some(6));

    rig.run(20, Some(&healthy));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert!(record.resumed);
    assert_eq!(record.drain_ran_s, 10);
    assert_eq!(record.result, OperationResult::Completed);
}

#[test]
fn the_resume_gap_is_measured_once_at_the_first_time_sync() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 400), Some(EPOCH - 300));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(None);
    rig.run(310, None);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );

    rig.run(1, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[test]
fn resuming_gives_up_when_levels_stay_unavailable_for_the_whole_window() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 400), Some(EPOCH - 300));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(None);
    rig.run(599, None);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );

    rig.run(1, None);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );
}

#[test]
fn a_failed_return_pump_forces_the_uv_off() {
    let mut rig = Rig::new();
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Return);

    rig.tick(Some(&healthy()));

    let channels = rig.service.status().channels;
    assert!(!rig.pins()[PumpId::Uv.index()]);
    assert_eq!(channels.uv.blocked_by, Some(Blocker::ReturnOff));
    assert_eq!(channels.return_pump.fault, Some(ChannelFault::OutputError));
}

#[test]
fn uncontrolled_channels_report_their_fail_safe_state() {
    let (service, _store) = service_and_store();
    let status = service.status();

    assert!(status.channels.return_pump.on);
    assert!(status.channels.uv.on);
    assert!(!status.channels.makeup.on);
    assert!(!status.channels.drain.on);

    let telemetry = service.pump_telemetry();
    assert!(telemetry.return_pump && telemetry.uv);
    assert!(!telemetry.makeup && !telemetry.drain);
}

#[test]
fn a_long_outage_suspends_until_resumed_over_the_api() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    assert_eq!(
        rig.service.resume(OperationKind::TopOff),
        Err(StartError::Conflict)
    );
    rig.service
        .resume(OperationKind::WaterChange)
        .expect("suspended");

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);
    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
}

#[test]
fn resuming_a_water_change_does_not_need_a_ready_reservoir() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let reservoir_low = snapshot(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]);

    rig.tick(Some(&reservoir_low));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    assert_eq!(rig.service.resume(OperationKind::WaterChange), Ok(()));
    rig.tick(Some(&reservoir_low));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[test]
fn cancelling_a_suspended_operation_records_the_reboot() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(Arc::clone(&store), TestClock::synced(EPOCH), None);

    rig.tick(Some(&healthy()));
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");

    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::InterruptedByReboot)
    );
    assert_eq!(store.get(PumpRecord::Checkpoint), None);
}

#[test]
fn resuming_waits_for_time_and_gives_up_after_the_window() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 20), Some(EPOCH - 10));
    let mut rig = Rig::build(store, TestClock::unsynced(), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.run(599, Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );
    assert!(!rig.pins()[3]);

    rig.run(1, Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );
}

fn sunday_ten_am() -> CivilTime {
    CivilTime {
        minute: 0,
        hour: 10,
        day: 7,
        month: 6,
        weekday: 0,
    }
}

fn schedule_top_off(service: &Service) {
    schedule_top_off_on(service);
}

fn schedule_top_off_on<S: PumpStore, C: Clock>(service: &PumpService<S, C>) {
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

#[test]
fn a_scheduled_window_fires_once_in_its_minute() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.run(1, Some(&healthy));
    assert!(!rig.pins()[2]);
}

#[test]
fn a_window_during_another_operation_is_skipped_as_busy() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    assert_eq!(
        rig.service.status().last_skipped,
        Some(SkippedWindow {
            kind: OperationKind::TopOff,
            index: 0,
            reason: SkipReason::Busy,
            blocked_by: None,
            at: Some(EPOCH + 1),
        })
    );
}

#[test]
fn a_blocked_window_is_skipped_with_its_blocker() {
    let mut rig = Rig::new();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);
    schedule_top_off(&rig.service);

    rig.tick(Some(&empty_reservoir));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&empty_reservoir));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::ReservoirEmpty));
}

fn spawn_loop(levels: &InMemoryWaterLevelStore) -> (Arc<Service>, RecordingOutputs, Arc<Notify>) {
    let (service, outputs, republish, _heartbeat) = spawn_loop_with_heartbeat(levels.subscribe());
    (service, outputs, republish)
}

fn spawn_loop_with_heartbeat(
    levels: watch::Receiver<Option<LevelSample>>,
) -> (Arc<Service>, RecordingOutputs, Arc<Notify>, Arc<AtomicU32>) {
    let service = service_on(
        &Arc::new(VolatilePumpStore::default()),
        TestClock::synced(EPOCH),
    );
    let outputs = RecordingOutputs::default();
    let republish = Arc::new(Notify::new());
    let heartbeat = Arc::new(AtomicU32::new(0));

    tokio::task::spawn_local(pump_loop(
        PumpController::new(outputs.clone(), Arc::clone(&service), timing()),
        levels,
        service.change_signal(),
        Arc::clone(&republish),
        timing(),
        Arc::clone(&heartbeat),
    ));

    (service, outputs, republish, heartbeat)
}

#[tokio::test(start_paused = true)]
async fn the_loop_treats_quiet_level_sensors_as_unavailable() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let (service, _outputs, _republish) = spawn_loop(&levels);

            levels.publish(healthy());
            tokio::time::sleep(Duration::from_millis(100)).await;
            assert_eq!(service.status().channels.makeup.blocked_by, None);

            tokio::time::sleep(Duration::from_millis(1500)).await;
            assert_eq!(
                service.status().channels.makeup.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_level_sample_delivered_late_is_judged_by_its_measurement_time() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let receiver = levels.subscribe();
            levels.publish(healthy());
            tokio::time::sleep(Duration::from_secs(2)).await;

            let (service, outputs, _republish, _heartbeat) = spawn_loop_with_heartbeat(receiver);
            tokio::time::sleep(Duration::from_millis(10)).await;

            assert_eq!(
                service.start_water_change(PLAN),
                Err(StartError::Blocked(Blocker::LevelsUnavailable))
            );
            tokio::time::sleep(Duration::from_millis(1500)).await;
            assert!(!outputs.state()[PumpId::Drain.index()]);
            assert_eq!(
                service.status().channels.drain.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_api_change_is_applied_without_waiting_for_the_ticker() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let (service, outputs, republish) = spawn_loop(&levels);

            levels.publish(healthy());
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert!(outputs.state()[1]);
            tokio::time::timeout(Duration::from_millis(10), republish.notified())
                .await
                .expect("startup republished");

            service.set_override(PumpId::Uv, None).expect("valid");
            tokio::time::sleep(Duration::from_millis(10)).await;

            assert!(!outputs.state()[1]);
            tokio::time::timeout(Duration::from_millis(10), republish.notified())
                .await
                .expect("republished");
        })
        .await;
}

struct EraseFails(Arc<VolatilePumpStore>);

impl PumpStore for EraseFails {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.0.load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.0.store(record, bytes)
    }

    fn erase(&self, _record: PumpRecord) -> Result<(), &'static str> {
        Err("flash is stuck")
    }
}

#[test]
fn a_failed_checkpoint_erase_does_not_resurrect_the_operation() {
    let store = Arc::new(VolatilePumpStore::default());
    let service = PumpService::new(
        PumpSettings::DEFAULT,
        EraseFails(Arc::clone(&store)),
        TestClock::synced(EPOCH),
        None,
    );

    service.observe(Some(HEALTHY), Duration::ZERO, Instant::now());
    service.start_top_off(60).expect("idle");
    service.cancel(OperationKind::TopOff).expect("cancelled");

    let restarted = service_on(&store, TestClock::synced(EPOCH));
    assert_eq!(restarted.status().operation, OperationStatus::IDLE);
}

#[test]
fn the_return_lockout_does_not_span_a_gap_in_level_data() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve]);

    rig.tick(Some(&healthy));
    rig.run(1, Some(&low));
    rig.run(20, Some(&healthy));
    rig.run(5, None);

    rig.run(30, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn operation_time_does_not_accrue_while_the_drain_output_fails() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Drain);
    rig.tick(Some(&healthy));
    rig.run(10, Some(&healthy));

    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(status.operation.drain_ran_s, 0);
    assert_eq!(status.channels.drain.fault, Some(ChannelFault::OutputError));
}

#[test]
fn an_unsynced_return_override_is_not_reported_as_in_force() {
    let store = Arc::new(VolatilePumpStore::default());
    let stored = Some(StoredOverride {
        expires_at: Some(EPOCH + 600),
    });
    store.put(
        PumpRecord::Overrides,
        serde_json::to_vec(&[stored, stored]).expect("encode"),
    );
    let mut rig = Rig::build(store, TestClock::unsynced(), None);

    rig.tick(Some(&healthy()));

    let channels = rig.service.status().channels;
    assert_eq!(channels.return_pump.mode, ChannelMode::Auto);
    assert!(channels.return_pump.on);
    assert_eq!(channels.uv.mode, ChannelMode::ManualOff);
}

#[test]
fn blocker_changes_alone_do_not_rewrite_the_runtime_record() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let overflow = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::MainTank,
    ]);

    rig.tick(Some(&healthy));
    rig.store.erase(PumpRecord::Runtime).expect("erased");

    rig.run(1, Some(&overflow));
    rig.run(1, Some(&healthy));
    rig.run(1, Some(&overflow));

    assert_eq!(rig.store.get(PumpRecord::Runtime), None);
}

#[test]
fn a_window_due_on_the_first_tick_sees_the_current_levels() {
    let mut rig = Rig::new();
    schedule_top_off(&rig.service);
    rig.clock.set_civil(Some(sunday_ten_am()));

    rig.tick(Some(&healthy()));

    assert!(rig.pins()[2]);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn a_dry_sump_does_not_end_the_drain_and_the_refill_waits_out_the_lockout() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]);

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, true]);

    rig.run(3, Some(&low));
    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Settling);
    assert_eq!(status.operation.drain_ran_s, 3);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );

    rig.run(2, Some(&low));
    assert_eq!(rig.pins(), [false, false, true, false]);
    assert_eq!(
        rig.service.status().channels.return_pump.blocked_by,
        Some(Blocker::SumpLow)
    );

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, true, false]);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::Completed)
    );
}

#[test]
fn a_water_change_refill_into_a_full_sump_waits_for_the_return_pump() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let full = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpReturnLevel,
    ]);

    rig.tick(Some(&full));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&full));
    rig.run(5, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(30, Some(&full));
    let status = rig.service.status();
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(status.operation.state, OperationState::Refilling);
    assert_eq!(status.operation.refill_ran_s, 0);
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpFull));

    rig.run(2, Some(&healthy));
    assert_eq!(rig.pins(), [true, true, true, false]);

    rig.run(1, Some(&full));
    assert_eq!(rig.pins(), [true, true, false, false]);
    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::SumpFull);
    assert_eq!(record.refill_ran_s, 2);
}

#[test]
fn a_resumed_settling_pause_keeps_the_return_pump_off() {
    let mut operation = Operation::start(
        OperationPlan::water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 30,
            refill_s: 10,
        }),
        Some(EPOCH - 200),
    );
    operation.advance(10_000, 10_000, Some(HEALTHY));
    operation.advance(5000, 5000, Some(HEALTHY));
    assert_eq!(operation.phase, Phase::Settling);
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, operation, Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(Some(&healthy()));

    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Settling);
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
}

#[test]
fn an_empty_reservoir_stops_the_refill() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.run(1, Some(&empty_reservoir));
    assert!(!rig.pins()[2]);
    assert_eq!(
        rig.service
            .status()
            .last_top_off
            .map(|record| record.result),
        Some(OperationResult::ReservoirEmpty)
    );
}

#[test]
fn an_automatic_resume_does_not_credit_the_elapsed_gap() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 200), Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.tick(None);
    rig.now += Duration::from_secs(300);
    rig.tick(Some(&healthy()));

    let operation = rig.service.status().operation;
    assert_eq!(operation.state, OperationState::Draining);
    assert_eq!(operation.drain_ran_s, 4);
}

#[test]
fn an_api_resume_does_not_credit_the_elapsed_gap() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 4000), Some(EPOCH - 3600));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service
        .resume(OperationKind::WaterChange)
        .expect("suspended");
    rig.now += Duration::from_secs(300);
    rig.tick(Some(&healthy));

    let operation = rig.service.status().operation;
    assert_eq!(operation.state, OperationState::Draining);
    assert_eq!(operation.drain_ran_s, 4);
}

type Log = Arc<Mutex<Vec<String>>>;

struct LogStore {
    inner: Arc<VolatilePumpStore>,
    log: Log,
    failing: Option<PumpRecord>,
}

impl LogStore {
    fn new(log: &Log) -> Self {
        Self {
            inner: Arc::new(VolatilePumpStore::default()),
            log: Arc::clone(log),
            failing: None,
        }
    }

    fn failing(record: PumpRecord) -> Self {
        Self {
            failing: Some(record),
            ..Self::new(&Log::default())
        }
    }
}

impl PumpStore for LogStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.inner.load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.log
            .lock()
            .unwrap()
            .push(format!("store {}", record.key()));

        if self.failing == Some(record) {
            return Err("flash is full");
        }

        self.inner.store(record, bytes)
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        self.log
            .lock()
            .unwrap()
            .push(format!("erase {}", record.key()));
        self.inner.erase(record)
    }
}

struct LogOutputs {
    log: Log,
}

impl PumpOutputs for LogOutputs {
    fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
        self.log.lock().unwrap().push(format!(
            "set {} {}",
            id.name(),
            if on { "on" } else { "off" }
        ));
        Ok(())
    }
}

fn position(log: &Log, entry: &str) -> usize {
    log.lock()
        .unwrap()
        .iter()
        .position(|logged| logged == entry)
        .unwrap_or_else(|| panic!("{entry} missing from {:?}", log.lock().unwrap()))
}

type LoggedService = PumpService<LogStore, Arc<TestClock>>;

fn logged_service(log: &Log) -> (Arc<LoggedService>, Arc<TestClock>) {
    let clock = TestClock::synced(EPOCH);
    let service = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        LogStore::new(log),
        Arc::clone(&clock),
        None,
    ));

    (service, clock)
}

#[test]
fn completion_replaces_the_active_checkpoint_before_archiving_history() {
    let log = Log::default();
    let (service, _clock) = logged_service(&log);

    service.observe(Some(HEALTHY), Duration::ZERO, Instant::now());
    service.start_top_off(60).expect("idle");
    log.lock().unwrap().clear();
    service.cancel(OperationKind::TopOff).expect("cancelled");

    assert!(position(&log, "store pump_op") < position(&log, "store pump_last"));
    assert!(position(&log, "store pump_last") < position(&log, "erase pump_op"));
}

#[test]
fn an_overflow_stops_the_makeup_pump_before_the_flash_writes() {
    let log = Log::default();
    let (service, clock) = logged_service(&log);
    let mut controller = PumpController::new(
        LogOutputs {
            log: Arc::clone(&log),
        },
        Arc::clone(&service),
        timing(),
    );
    let healthy = healthy();
    let overflow = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::MainTank,
    ]);
    let mut now = Instant::now();

    controller.tick(Some(&healthy), now);
    service.start_top_off(60).expect("idle");
    now += Duration::from_secs(1);
    clock.advance(1);
    controller.tick(Some(&healthy), now);

    log.lock().unwrap().clear();
    now += Duration::from_secs(1);
    clock.advance(1);
    controller.tick(Some(&overflow), now);

    let makeup_off = position(&log, "set makeup off");
    assert!(makeup_off < position(&log, "store pump_last"));
    assert!(makeup_off < position(&log, "erase pump_op"));
}

#[test]
fn a_scheduled_start_writes_its_checkpoint_before_switching_the_relay() {
    let log = Log::default();
    let (service, clock) = logged_service(&log);
    schedule_top_off_on(&service);
    let mut controller = PumpController::new(
        LogOutputs {
            log: Arc::clone(&log),
        },
        Arc::clone(&service),
        timing(),
    );
    let healthy = healthy();
    let now = Instant::now();

    controller.tick(Some(&healthy), now);
    clock.set_civil(Some(sunday_ten_am()));
    log.lock().unwrap().clear();
    controller.tick(Some(&healthy), now + Duration::from_secs(1));

    assert!(position(&log, "store pump_op") < position(&log, "set makeup on"));
}

#[test]
fn a_failed_override_write_is_reported_and_rolled_back() {
    let service = PumpService::new(
        PumpSettings::DEFAULT,
        LogStore::failing(PumpRecord::Overrides),
        TestClock::synced(EPOCH),
        None,
    );

    assert_eq!(
        service.set_override(PumpId::Uv, None),
        Err(OverrideError::NotPersisted("flash is full"))
    );
    assert_eq!(service.status().channels.uv.mode, ChannelMode::Auto);

    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::Overrides,
        serde_json::to_vec(&[Some(StoredOverride { expires_at: None }), None]).expect("encode"),
    );
    let service = PumpService::new(
        PumpSettings::DEFAULT,
        LogStore {
            inner: store,
            log: Log::default(),
            failing: Some(PumpRecord::Overrides),
        },
        TestClock::synced(EPOCH),
        None,
    );

    assert_eq!(service.clear_override(PumpId::Uv), Ok(()));
    assert_eq!(
        service.clear_override(PumpId::Return),
        Err(OverrideError::NotPersisted("flash is full"))
    );
    assert_eq!(
        service.status().channels.return_pump.mode,
        ChannelMode::ManualOff
    );
}

#[test]
fn a_timed_override_without_time_sync_never_fails_on_persistence() {
    let service = PumpService::new(
        PumpSettings::DEFAULT,
        LogStore::failing(PumpRecord::Overrides),
        TestClock::unsynced(),
        None,
    );

    let status = service
        .set_override(PumpId::Uv, Some(600))
        .expect("volatile");
    assert_eq!(status.mode, ChannelMode::ManualOff);
}

#[test]
fn a_runtime_reset_credits_only_the_time_after_it() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    let start = rig.now;
    rig.now += Duration::from_secs(10);
    rig.service
        .reset_runtime_at(PumpId::Uv, start + Duration::from_secs(8));
    rig.tick(Some(&healthy));

    let status = rig.service.status().channels;
    assert_eq!(status.uv.runtime_s, 2);
    assert_eq!(status.return_pump.runtime_s, 10);
}

fn set_refill_stop(service: &Service, role: WaterLevelRole) {
    service
        .apply_settings(PumpSettingsPatch {
            refill_stop_sensor: Some(Some(role)),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

#[test]
fn the_configured_refill_stop_sensor_ends_a_top_off() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let full = snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::SumpBiological,
    ]);
    set_refill_stop(&rig.service, WaterLevelRole::SumpBiological);

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);

    rig.run(1, Some(&full));
    assert!(!rig.pins()[2]);

    let status = rig.service.status();
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpFull));
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::SumpFull)
    );
}

#[test]
fn a_top_off_does_not_start_into_a_full_sump() {
    let (service, _store) = service_and_store();
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    observe(&service, Some(full));

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::SumpFull))
    );
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn refills_do_not_start_while_the_return_pump_is_forced_off() {
    let (service, _store) = service_and_store();
    observe(&service, Some(HEALTHY));
    service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");

    assert_eq!(
        service.start_top_off(60),
        Err(StartError::Blocked(Blocker::ReturnOff))
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::ReturnOff))
    );

    service.clear_override(PumpId::Return).expect("overridable");
    assert_eq!(service.start_top_off(60), Ok(()));
}

#[test]
fn a_running_refill_stalls_while_the_return_pump_is_forced_off() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));
    rig.service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");
    rig.run(5, Some(&healthy));

    let status = rig.service.status();
    assert_eq!(rig.pins(), [false, false, false, false]);
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::ReturnOff));
    assert_eq!(status.operation.refill_ran_s, 1);
}

#[test]
fn a_scheduled_top_off_is_skipped_while_the_return_pump_is_forced_off() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.service
        .set_override(PumpId::Return, Some(600))
        .expect("valid");
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::ReturnOff));
}

#[test]
fn makeup_refills_a_drained_sump_during_the_return_lockout() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve]);

    rig.tick(Some(&healthy));
    rig.run(1, Some(&low));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&healthy));

    assert_eq!(rig.pins(), [false, false, true, false]);
}

const HOUR: u64 = 3600;

fn drain_briefly(service: &Service) {
    observe(service, Some(HEALTHY));
    service.record(PumpId::Drain, true, None, None);
    service.observe(Some(HEALTHY), Duration::from_millis(500), Instant::now());
}

fn finish_a_water_change(service: &Service) {
    observe(service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");
    drain_briefly(service);
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
}

fn cancel_a_water_change_before_it_drains(service: &Service) {
    observe(service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");
    observe(service, Some(HEALTHY));
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
}

#[test]
fn a_water_change_cancelled_before_draining_does_not_hold_back_the_next() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    cancel_a_water_change_before_it_drains(&service);
    assert_eq!(
        service
            .status()
            .last_water_change
            .map(|record| (record.result, record.drain_ran_s)),
        Some((OperationResult::Aborted, 0))
    );

    clock.advance(60);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn an_undrained_water_change_keeps_the_gap_of_the_drained_one_before() {
    let clock = TestClock::synced(EPOCH);
    let store = Arc::new(VolatilePumpStore::default());
    let service = service_on(&store, Arc::clone(&clock));
    finish_a_water_change(&service);
    assert_eq!(
        service
            .status()
            .last_water_change
            .map(|record| record.drain_ran_s),
        Some(0),
        "half a second of draining rounds down in the record but still counts"
    );

    clock.advance(13 * HOUR);
    cancel_a_water_change_before_it_drains(&service);
    service
        .apply_settings(PumpSettingsPatch {
            min_water_change_gap_h: Some(24),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");

    let reloaded = service_on(&store, Arc::clone(&clock));
    observe(&reloaded, Some(HEALTHY));
    assert_eq!(
        reloaded.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );

    clock.advance(11 * HOUR);
    assert_eq!(reloaded.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_water_change_waits_out_the_gap_after_the_previous_one() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    finish_a_water_change(&service);

    clock.advance(12 * HOUR - 1);
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
    assert_eq!(service.start_top_off(60), Ok(()));
    service.cancel(OperationKind::TopOff).expect("cancelled");

    clock.advance(1);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_zero_gap_disables_the_water_change_limit() {
    let (service, _store) = service_and_store();
    service
        .apply_settings(PumpSettingsPatch {
            min_water_change_gap_h: Some(0),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
    finish_a_water_change(&service);

    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn the_water_change_gap_needs_both_times() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    finish_a_water_change(&service);

    clock.set_epoch(None);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
    drain_briefly(&service);
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");

    clock.set_epoch(Some(EPOCH + 60));
    assert_eq!(service.start_water_change(PLAN), Ok(()));
}

#[test]
fn a_water_change_recorded_in_the_future_is_too_recent() {
    let clock = TestClock::synced(EPOCH);
    let service = service_on(&Arc::new(VolatilePumpStore::default()), Arc::clone(&clock));
    finish_a_water_change(&service);

    clock.set_epoch(Some(EPOCH - HOUR));
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn a_scheduled_water_change_too_soon_is_skipped() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.service
        .apply_settings(PumpSettingsPatch {
            water_change: Some(vec![crate::domain::pump::WaterChangeWindow {
                cron: "0 10 * * sun".into(),
                drain_s: 10,
                settle_s: 0,
                refill_s: 10,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    drain_briefly(&rig.service);
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.kind, OperationKind::WaterChange);
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::RecentWaterChange));
}

#[test]
fn a_scheduled_water_change_is_skipped_while_the_reservoir_is_low() {
    let mut rig = Rig::new();
    let reservoir_low = snapshot(&[WaterLevelRole::SumpPump, WaterLevelRole::SumpReserve]);
    rig.service
        .apply_settings(PumpSettingsPatch {
            water_change: Some(vec![crate::domain::pump::WaterChangeWindow {
                cron: "0 10 * * sun".into(),
                drain_s: 10,
                settle_s: 0,
                refill_s: 10,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");

    rig.tick(Some(&reservoir_low));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&reservoir_low));

    let status = rig.service.status();
    assert_eq!(status.operation.state, OperationState::Idle);
    let skipped = status.last_skipped.expect("skipped");
    assert_eq!(skipped.kind, OperationKind::WaterChange);
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::ReservoirLow));
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn an_operation_resumes_automatically_only_once() {
    let store = Arc::new(VolatilePumpStore::default());
    let mut operation = interrupted_drain(EPOCH - 200);
    operation.resume();
    seed_checkpoint(&store, operation, Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    rig.service
        .resume(OperationKind::WaterChange)
        .expect("suspended");
    rig.tick(Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[tokio::test(start_paused = true)]
async fn the_loop_beats_its_heartbeat_every_tick() {
    LocalSet::new()
        .run_until(async {
            let levels = InMemoryWaterLevelStore::new();
            let (_service, _outputs, _republish, heartbeat) =
                spawn_loop_with_heartbeat(levels.subscribe());

            tokio::time::sleep(Duration::from_millis(3500)).await;

            assert!(
                heartbeat.load(Ordering::Relaxed) >= 4,
                "{}",
                heartbeat.load(Ordering::Relaxed)
            );
        })
        .await;
}

fn full_sump() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
        WaterLevelRole::SumpReturnLevel,
    ])
}

#[test]
fn a_refill_stuck_on_a_wet_stop_sensor_times_out() {
    let mut rig = Rig::new();
    let full = full_sump();

    rig.tick(Some(&full));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&full));
    rig.run(5, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(304, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );
    let checkpoint: Checkpoint = decode(&rig.store, PumpRecord::Checkpoint).expect("stored");
    assert!(
        checkpoint.operation.refill_wall_ms >= 290_000,
        "{}",
        checkpoint.operation.refill_wall_ms
    );

    rig.run(1, Some(&full));
    let status = rig.service.status();
    assert_eq!(status.operation, OperationStatus::IDLE);
    let record = status.last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::RefillTimeout);
    assert_eq!(record.refill_ran_s, 0);
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        decode::<Option<Checkpoint>>(&rig.store, PumpRecord::Checkpoint).flatten(),
        None
    );
}

#[test]
fn a_resumed_refill_continues_its_wall_time_budget() {
    let full = LevelConditions {
        sump_full: true,
        ..HEALTHY
    };
    let mut operation = Operation::start(
        OperationPlan::water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 60,
        }),
        Some(EPOCH - 400),
    );
    operation.advance(10_000, 10_000, Some(HEALTHY));
    operation.advance(0, 0, Some(HEALTHY));
    assert_eq!(operation.phase, Phase::Refilling);
    operation.advance(0, 350_000, Some(full));
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, operation, Some(EPOCH - 100));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    let full = full_sump();

    rig.tick(None);
    rig.now += Duration::from_secs(300);
    rig.tick(Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(10, Some(&full));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(1, Some(&full));
    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.result, OperationResult::RefillTimeout);
    assert!(record.resumed);
}

fn reservoir_patch(drain_lph: u32, reservoir_l: Option<u32>) -> PumpSettingsPatch {
    PumpSettingsPatch {
        flow_lph: Some(crate::domain::pump::FlowRates {
            makeup: Some(600),
            drain: Some(drain_lph),
        }),
        reservoir_ready_l: Some(reservoir_l),
        ..PumpSettingsPatch::default()
    }
}

#[test]
fn a_water_change_draining_more_than_the_ready_reservoir_is_refused() {
    let (service, _store) = service_and_store();
    service
        .apply_settings(reservoir_patch(1500, Some(50)))
        .expect("valid");
    observe(&service, Some(HEALTHY));

    assert_eq!(
        service.start_water_change(WaterChangePlan {
            drain_s: 121,
            ..PLAN
        }),
        Err(StartError::Invalid(
            "the planned drain exceeds the water the reservoir holds"
        ))
    );
    assert_eq!(service.status().operation, OperationStatus::IDLE);
    assert_eq!(service.status().last_water_change, None);

    assert_eq!(
        service.start_water_change(WaterChangePlan {
            drain_s: 120,
            ..PLAN
        }),
        Ok(())
    );
}

#[test]
fn a_settings_change_that_would_overdraw_a_stored_window_is_refused() {
    let (service, store) = service_and_store();
    service
        .apply_settings(PumpSettingsPatch {
            water_change: Some(vec![crate::domain::pump::WaterChangeWindow {
                cron: "0 10 * * sun".into(),
                drain_s: 120,
                settle_s: 0,
                refill_s: 300,
            }]),
            ..reservoir_patch(1500, Some(50))
        })
        .expect("valid");
    let saved = service.settings();

    for patch in [
        reservoir_patch(1600, Some(50)),
        reservoir_patch(1500, Some(49)),
    ] {
        assert_eq!(
            service.apply_settings(patch),
            Err(PumpSettingsError::Invalid(
                "a water-change window drains more than reservoir_ready_l holds"
            ))
        );
        assert_eq!(service.settings(), saved);
        assert_eq!(
            decode::<PumpSettings>(&store, PumpRecord::Settings),
            Some(saved.clone())
        );
    }

    let cleared = service
        .apply_settings(reservoir_patch(1600, None))
        .expect("no reservoir volume, no check");
    assert_eq!(cleared.reservoir_ready_l, None);
}

#[test]
fn an_automatic_resume_records_itself_before_switching_the_relays() {
    let log = Log::default();
    let store = LogStore::new(&log);
    seed_checkpoint(
        &store.inner,
        interrupted_drain(EPOCH - 200),
        Some(EPOCH - 100),
    );
    let service = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        store,
        TestClock::synced(EPOCH),
        None,
    ));
    let mut controller = PumpController::new(
        LogOutputs {
            log: Arc::clone(&log),
        },
        Arc::clone(&service),
        timing(),
    );

    controller.tick(Some(&healthy()), Instant::now());

    assert!(position(&log, "store pump_op") < position(&log, "set drain on"));
}

fn configure_flows(service: &Service) {
    service
        .apply_settings(PumpSettingsPatch {
            flow_lph: Some(crate::domain::pump::FlowRates {
                makeup: Some(600),
                drain: Some(1500),
            }),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

#[test]
fn a_water_change_that_refilled_too_little_is_recorded_incomplete() {
    let (sender, mut events) = mpsc::unbounded_channel();
    let mut rig = Rig::build(
        Arc::new(VolatilePumpStore::default()),
        TestClock::synced(EPOCH),
        Some(sender),
    );
    configure_flows(&rig.service);
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 3,
        })
        .expect("idle");
    rig.tick(Some(&healthy));
    rig.run(16, Some(&healthy));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.drain_ran_s, 10);
    assert_eq!(record.refill_ran_s, 3);
    assert_eq!(record.result, OperationResult::Incomplete);
    let stored: crate::domain::pump::LastOperations =
        decode(&rig.store, PumpRecord::LastOperations).expect("stored");
    assert_eq!(
        stored.water_change.map(|record| record.result),
        Some(OperationResult::Incomplete)
    );
    let event = events.try_recv().expect("an event");
    assert_eq!(event.record.result, OperationResult::Incomplete);
}

#[test]
fn a_water_change_that_stopped_on_the_full_sump_is_not_graded() {
    let mut rig = Rig::new();
    configure_flows(&rig.service);
    let healthy = healthy();
    let full = full_sump();

    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 10,
            settle_s: 0,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));
    rig.run(11, Some(&healthy));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Refilling
    );

    rig.run(2, Some(&healthy));
    rig.run(1, Some(&full));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.refill_ran_s, 3);
    assert_eq!(record.result, OperationResult::SumpFull);
}

#[test]
fn a_water_change_refill_that_runs_out_its_time_with_the_sump_low_is_incomplete() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let low = snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]);

    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.tick(Some(&healthy));
    rig.run(5, Some(&healthy));
    rig.run(5, Some(&low));

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.refill_ran_s, PLAN.refill_s);
    assert_eq!(record.result, OperationResult::Incomplete);
}

#[test]
fn a_top_off_is_never_graded_incomplete() {
    let mut rig = Rig::new();
    configure_flows(&rig.service);
    let low = snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]);

    rig.tick(Some(&low));
    rig.service.start_top_off(3).expect("idle");
    rig.tick(Some(&low));
    rig.run(4, Some(&low));

    assert_eq!(
        rig.service
            .status()
            .last_top_off
            .map(|record| record.result),
        Some(OperationResult::Completed)
    );
}

fn low_sump() -> WaterLevelSnapshot {
    snapshot(&[WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady])
}

fn starved(rig: &Rig) -> bool {
    rig.service.status().circulation_starved
}

fn lockout_cycle(rig: &mut Rig) {
    rig.run(1, Some(&low_sump()));
    rig.run(40, Some(&healthy()));
}

#[test]
fn three_sump_low_lockouts_within_ten_minutes_flag_starved_circulation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));

    lockout_cycle(&mut rig);
    lockout_cycle(&mut rig);
    assert!(!starved(&rig));

    rig.run(1, Some(&low_sump()));
    assert!(starved(&rig));
    assert!(rig.service.pump_telemetry().circulation_starved);
}

#[test]
fn starved_circulation_clears_after_ten_wet_minutes() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));
    lockout_cycle(&mut rig);
    lockout_cycle(&mut rig);
    rig.run(1, Some(&low_sump()));
    assert!(starved(&rig));

    rig.run(600, Some(&healthy()));
    assert!(starved(&rig));

    rig.run(1, Some(&healthy()));
    assert!(!starved(&rig));
}

#[test]
fn lockouts_spread_over_more_than_ten_minutes_are_not_starvation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));

    for _ in 0..4 {
        lockout_cycle(&mut rig);
        rig.run(260, Some(&healthy()));
    }

    assert!(!starved(&rig));
}

#[test]
fn a_sump_that_stays_low_for_ten_minutes_starves_the_circulation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));

    rig.run(600, Some(&low_sump()));
    assert!(!starved(&rig));

    rig.run(1, Some(&low_sump()));
    assert!(starved(&rig));
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn a_low_sump_during_an_operation_is_not_starvation() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.service
        .start_water_change(WaterChangePlan {
            drain_s: 700,
            settle_s: 0,
            refill_s: 60,
        })
        .expect("idle");
    rig.tick(Some(&healthy));

    for _ in 0..3 {
        rig.run(1, Some(&low_sump()));
        rig.run(10, Some(&healthy));
    }
    rig.run(650, Some(&low_sump()));

    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
    assert!(!starved(&rig));
}

#[test]
fn missing_levels_neither_raise_nor_clear_starvation() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));
    rig.run(300, Some(&low_sump()));
    rig.run(400, None);
    rig.run(300, Some(&low_sump()));
    assert!(!starved(&rig));

    rig.run(301, Some(&low_sump()));
    assert!(starved(&rig));

    rig.run(300, Some(&healthy()));
    rig.run(700, None);
    rig.run(300, Some(&healthy()));
    assert!(starved(&rig));
}

#[test]
fn raising_starvation_republishes_the_status() {
    let mut rig = Rig::new();
    rig.run(1, Some(&healthy()));
    rig.run(600, Some(&low_sump()));

    rig.now += Duration::from_secs(1);
    rig.clock.advance(1);
    assert!(rig.tick(Some(&low_sump())));
    assert!(starved(&rig));
}

fn fall_back_twice(rig: &mut Rig, step_back_s: u64) {
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
    assert!(rig.pins()[2]);
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.clock.set_civil(None);
    rig.run(120, Some(&healthy));

    let now = rig.clock.epoch_seconds().expect("synced");
    rig.clock.set_epoch(Some(now - step_back_s));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
}

#[test]
fn a_window_does_not_fire_twice_for_the_same_local_minute() {
    let mut rig = Rig::new();
    fall_back_twice(&mut rig, 0);

    assert!(!rig.pins()[2]);
    assert_eq!(rig.service.status().operation, OperationStatus::IDLE);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn a_backward_clock_step_does_not_refire_a_window() {
    let mut rig = Rig::new();
    fall_back_twice(&mut rig, 300);

    assert!(!rig.pins()[2]);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn the_same_local_minute_fires_again_a_day_later() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_top_off(&rig.service);

    rig.tick(Some(&healthy));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.clock.set_civil(None);

    let now = rig.clock.epoch_seconds().expect("synced");
    rig.clock.set_epoch(Some(now + 7 * 86_400));
    rig.clock.set_civil(Some(CivilTime {
        day: 14,
        ..sunday_ten_am()
    }));
    rig.run(1, Some(&healthy));

    assert!(rig.pins()[2]);
}

#[test]
fn a_skipped_window_is_not_retried_in_the_repeated_hour() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);
    schedule_top_off(&rig.service);

    rig.tick(Some(&empty_reservoir));
    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&empty_reservoir));
    let skipped = rig.service.status().last_skipped.expect("skipped");
    rig.clock.set_civil(None);
    rig.run(3600, Some(&healthy));

    rig.clock.set_civil(Some(sunday_ten_am()));
    rig.run(1, Some(&healthy));

    assert!(!rig.pins()[2]);
    assert_eq!(rig.service.status().last_skipped, Some(skipped));
}

fn step_clock(rig: &Rig, seconds: i64) {
    let now = rig.clock.epoch_seconds().expect("synced");
    rig.clock
        .set_epoch(Some(now.saturating_add_signed(seconds)));
}

#[test]
fn a_timed_override_lasts_its_duration_when_the_clock_steps() {
    for jump in [-7200, -300, 300, 7200] {
        let mut rig = Rig::new();
        let healthy = healthy();
        rig.tick(Some(&healthy));
        rig.set_override(PumpId::Uv, Some(600));
        rig.run(100, Some(&healthy));

        step_clock(&rig, jump);
        rig.run(499, Some(&healthy));
        assert!(!rig.pins()[1], "{jump:+}");

        rig.run(1, Some(&healthy));
        assert!(rig.pins()[1], "{jump:+}");
        assert_eq!(
            decode::<[Option<StoredOverride>; 2]>(&rig.store, PumpRecord::Overrides),
            Some([None, None]),
            "{jump:+}"
        );
    }
}

#[test]
fn a_return_override_set_this_boot_survives_a_lost_time_sync() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.set_override(PumpId::Return, Some(120));
    rig.run(10, Some(&healthy));

    rig.clock.set_epoch(None);
    rig.run(10, Some(&healthy));
    assert_eq!(
        rig.service.status().channels.return_pump.mode,
        ChannelMode::ManualOff
    );
    assert!(!rig.pins()[0]);

    rig.run(100, Some(&healthy));
    assert!(rig.pins()[0]);
}

#[test]
fn a_timed_override_still_persists_its_wall_clock_expiry() {
    let (service, store) = service_and_store();

    service.set_override(PumpId::Uv, Some(600)).expect("valid");

    assert_eq!(
        decode::<[Option<StoredOverride>; 2]>(&store, PumpRecord::Overrides),
        Some([
            None,
            Some(StoredOverride {
                expires_at: Some(EPOCH + 600)
            })
        ])
    );
}

fn running_sump() -> WaterLevelSnapshot {
    snapshot(&[
        WaterLevelRole::SumpPump,
        WaterLevelRole::SumpReturnLevel,
        WaterLevelRole::SumpReserve,
        WaterLevelRole::ReservoirReady,
    ])
}

fn shorted() -> WaterLevelSnapshot {
    snapshot(&WaterLevelRole::ALL)
}

fn suspect(rig: &Rig) -> bool {
    rig.service.status().level_sensors_suspect
}

#[test]
fn an_unplugged_harness_is_treated_as_missing_levels() {
    let mut rig = Rig::new();
    set_refill_stop(&rig.service, WaterLevelRole::SumpMechanical);
    rig.run(5, Some(&running_sump()));
    rig.service.start_top_off(60).expect("idle");
    rig.run(1, Some(&running_sump()));
    assert!(rig.pins()[PumpId::Makeup.index()]);
    assert!(!suspect(&rig));

    let unplugged = WaterLevelSnapshot::dry();
    rig.now += Duration::from_millis(100);
    assert!(rig.tick(Some(&unplugged)));
    assert!(suspect(&rig));
    assert!(rig.service.pump_telemetry().level_sensors_suspect);
    assert_eq!(rig.pins(), [true, true, false, false]);

    let status = rig.service.status();
    assert_eq!(
        status.last_top_off.map(|record| record.result),
        Some(OperationResult::Aborted)
    );
    assert_eq!(
        status.channels.makeup.blocked_by,
        Some(Blocker::LevelsUnavailable)
    );
    assert_eq!(status.channels.return_pump.blocked_by, None);

    rig.run(900, Some(&unplugged));
    assert!(suspect(&rig));
    assert_eq!(rig.pins(), [true, true, false, false]);
    assert!(!starved(&rig));
    assert_eq!(
        rig.service.start_top_off(60),
        Err(StartError::Blocked(Blocker::LevelsUnavailable))
    );

    rig.run(1, Some(&running_sump()));
    assert!(!suspect(&rig));
    assert_eq!(rig.service.start_top_off(60), Ok(()));
}

#[test]
fn a_sump_drained_by_the_pumps_is_not_an_unplugged_harness() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.run(30, Some(&healthy()));
    rig.run(1, Some(&low_sump()));

    assert!(!suspect(&rig));
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn a_shorted_harness_refuses_a_water_change_and_keeps_the_return() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));

    rig.now += Duration::from_millis(100);
    assert!(rig.tick(Some(&shorted())));
    assert!(suspect(&rig));
    rig.run(10, Some(&shorted()));

    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::LevelsUnavailable))
    );

    let mut tank_dry = shorted();
    tank_dry.sensors[WaterLevelRole::MainTank.index()].state = WaterLevelState::Dry;
    rig.run(1, Some(&tank_dry));
    assert!(!suspect(&rig));
}

#[test]
fn a_harness_fault_is_reported_until_it_clears() {
    let mut rig = Rig::new();
    rig.run(1, Some(&running_sump()));
    assert!(!rig.service.pump_telemetry().level_sensors_suspect);

    rig.run(1, Some(&WaterLevelSnapshot::dry()));
    assert!(rig.service.pump_telemetry().level_sensors_suspect);

    rig.now += Duration::from_secs(1);
    assert!(rig.tick(Some(&running_sump())));
    assert!(!rig.service.pump_telemetry().level_sensors_suspect);
}

const DRAINED_HISTORY_WITH_AN_UNKNOWN_RESULT: &[u8] = br#"{"top_off":null,
    "water_change":{"kind":"water_change","started_at":1700000000,"drain_ran_s":3,
    "settle_ran_s":2,"refill_ran_s":4,"result":"exploded","resumed":false},
    "drained_at":1700000000}"#;

#[test]
fn the_gap_survives_an_unreadable_water_change_record() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::LastOperations,
        DRAINED_HISTORY_WITH_AN_UNKNOWN_RESULT.to_vec(),
    );
    let service = service_on(&store, TestClock::synced(EPOCH + 3600));
    observe(&service, Some(HEALTHY));

    assert_eq!(service.status().last_water_change, None);
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn one_unreadable_history_entry_keeps_the_others() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::LastOperations,
        br#"{"top_off":{"kind":"top_off","started_at":1,"drain_ran_s":0,"settle_ran_s":0,
            "refill_ran_s":60,"result":"exploded","resumed":false},
            "water_change":{"kind":"water_change","started_at":1700000000,"drain_ran_s":3,
            "settle_ran_s":2,"refill_ran_s":4,"result":"sump_full","resumed":false}}"#
            .to_vec(),
    );
    let service = service_on(&store, TestClock::synced(EPOCH + 3600));
    observe(&service, Some(HEALTHY));

    let status = service.status();
    assert_eq!(status.last_top_off, None);
    assert_eq!(
        status.last_water_change.map(|record| record.result),
        Some(OperationResult::SumpFull)
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn a_stored_override_without_an_expiry_is_dropped() {
    for (stored, uv_off) in [
        (&br#"[null,{}]"#[..], false),
        (br#"[null,{"expires_at":null}]"#, true),
        (br#"[null,{"expires_at":"soon"}]"#, false),
        (br#"[{},{"expires_at":null}]"#, true),
        (br#"[null,{"expires_at":null},null]"#, true),
        (br#"[{"expires_at":null}]"#, false),
    ] {
        let store = Arc::new(VolatilePumpStore::default());
        store.put(PumpRecord::Overrides, stored.to_vec());
        let service = service_on(&store, TestClock::synced(EPOCH));
        let status = service.status().channels;

        let label = String::from_utf8_lossy(stored);
        assert_eq!(status.uv.mode == ChannelMode::ManualOff, uv_off, "{label}");
    }
}

#[derive(Default)]
struct Warnings(Mutex<Vec<String>>);

struct Message<'a>(&'a mut String);

impl tracing::field::Visit for Message<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{value:?}");
        }
    }
}

impl tracing::Subscriber for Warnings {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() == tracing::Level::WARN {
            let mut message = String::new();
            event.record(&mut Message(&mut message));
            self.0.lock().unwrap().push(message);
        }
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

fn warnings_while(run: impl FnOnce()) -> Vec<String> {
    let warnings = Arc::new(Warnings::default());
    tracing::subscriber::with_default(Arc::clone(&warnings), run);
    let logged = warnings.0.lock().unwrap().clone();
    logged
}

#[test]
fn stored_settings_that_fail_validation_are_reported_before_the_fallback() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::Settings,
        br#"{"top_off":[],"water_change":[],"flow_lph":{},"resume_window_s":9999}"#.to_vec(),
    );

    let mut settings = None;
    let warnings = warnings_while(|| {
        settings = Some(service_on(&store, TestClock::synced(EPOCH)).settings());
    });

    assert_eq!(settings, Some(PumpSettings::DEFAULT));
    assert!(
        warnings.iter().any(|message| message.contains("invalid")),
        "{warnings:?}"
    );
}

fn schedule_two_minutes(service: &Service) {
    service
        .apply_settings(PumpSettingsPatch {
            top_off: Some(vec![TopOffWindow {
                cron: "0,30 10 * * *".into(),
                duration_s: 120,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
}

fn fire_at(rig: &mut Rig, epoch: u64, minute: u8, levels: &WaterLevelSnapshot) -> bool {
    rig.clock.set_epoch(Some(epoch));
    rig.clock.set_civil(Some(CivilTime {
        minute,
        ..sunday_ten_am()
    }));
    rig.now += Duration::from_secs(1);
    rig.tick(Some(levels));
    let started = rig.pins()[PumpId::Makeup.index()];
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");
    rig.clock.set_civil(None);
    rig.run(1, Some(levels));
    started
}

#[test]
fn a_window_matching_two_minutes_fires_each_once_across_a_fall_back() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_two_minutes(&rig.service);
    rig.tick(Some(&healthy));

    let fired: Vec<bool> = [(0, 0), (1800, 30), (3600, 0), (5400, 30)]
        .into_iter()
        .map(|(offset, minute)| fire_at(&mut rig, EPOCH + offset, minute, &healthy))
        .collect();

    assert_eq!(fired, [true, true, false, false]);
    assert_eq!(rig.service.status().last_skipped, None);
}

#[test]
fn a_minute_blocked_at_its_first_occurrence_is_not_retried_on_its_repeat() {
    let mut rig = Rig::new();
    let healthy = healthy();
    let empty_reservoir = snapshot(&[WaterLevelRole::SumpPump]);
    schedule_two_minutes(&rig.service);
    rig.tick(Some(&healthy));

    assert!(!fire_at(&mut rig, EPOCH, 0, &empty_reservoir));
    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert!(fire_at(&mut rig, EPOCH + 1800, 30, &healthy));
    assert!(!fire_at(&mut rig, EPOCH + 3600, 0, &healthy));
    assert!(!fire_at(&mut rig, EPOCH + 5400, 30, &healthy));

    assert_eq!(rig.service.status().last_skipped, Some(skipped));
}

#[test]
fn every_minute_of_a_dense_window_fires_only_once_across_a_fall_back() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.service
        .apply_settings(PumpSettingsPatch {
            top_off: Some(vec![TopOffWindow {
                cron: "* 10 * * *".into(),
                duration_s: 120,
            }]),
            ..PumpSettingsPatch::default()
        })
        .expect("valid");
    rig.tick(Some(&healthy));

    for minute in 0..60u8 {
        assert!(fire_at(
            &mut rig,
            EPOCH + u64::from(minute) * 60,
            minute,
            &healthy
        ));
    }

    for minute in 0..60u8 {
        assert!(!fire_at(
            &mut rig,
            EPOCH + 3600 + u64::from(minute) * 60,
            minute,
            &healthy
        ));
    }
}

fn reloaded(rig: &Rig) -> Arc<Service> {
    let service = service_on(&rig.store, Arc::clone(&rig.clock));
    observe(&service, Some(HEALTHY));
    service
}

#[test]
fn a_cancel_right_after_the_drain_switched_on_still_counts_for_the_gap() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.now += Duration::from_millis(40);
    rig.tick(Some(&healthy));
    assert!(rig.pins()[PumpId::Drain.index()]);

    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    rig.clock.advance(60);

    assert_eq!(
        rig.service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
    assert_eq!(
        reloaded(&rig).start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn a_cancel_before_the_drain_ever_switched_on_does_not_count_for_the_gap() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.service.start_water_change(PLAN).expect("idle");
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Drain);
    rig.now += Duration::from_millis(40);
    rig.tick(Some(&healthy));

    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    *rig.outputs.failing.lock().unwrap() = None;
    rig.clock.advance(60);

    assert_eq!(reloaded(&rig).start_water_change(PLAN), Ok(()));
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    assert_eq!(rig.service.start_water_change(PLAN), Ok(()));
}

const OLD_DRAINED_HISTORY: &[u8] = br#"{"top_off":null,
    "water_change":{"kind":"water_change","started_at":1700000000,"drain_ran_s":3,
    "settle_ran_s":2,"refill_ran_s":4,"result":"completed","resumed":false}}"#;

#[test]
fn an_old_history_keeps_its_drain_date_when_an_unsynced_cancel_overwrites_it() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(PumpRecord::LastOperations, OLD_DRAINED_HISTORY.to_vec());
    let clock = TestClock::synced(EPOCH + 100);
    let service = service_on(&store, Arc::clone(&clock));
    observe(&service, Some(HEALTHY));
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );

    clock.set_epoch(None);
    assert_eq!(service.start_water_change(PLAN), Ok(()));
    observe(&service, Some(HEALTHY));
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");

    clock.set_epoch(Some(EPOCH + 101));
    let reloaded = service_on(&store, Arc::clone(&clock));
    observe(&reloaded, Some(HEALTHY));
    assert_eq!(
        reloaded.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[derive(Default)]
struct FlakyStore {
    inner: VolatilePumpStore,
    refuse: std::sync::atomic::AtomicBool,
    invalidations: AtomicU32,
}

impl FlakyStore {
    fn refuse(&self, refuse: bool) {
        self.refuse.store(refuse, Ordering::Relaxed);
    }

    fn refusing(&self, record: PumpRecord) -> bool {
        record == PumpRecord::Checkpoint && self.refuse.load(Ordering::Relaxed)
    }

    fn holds_checkpoint(&self) -> bool {
        self.inner
            .get(PumpRecord::Checkpoint)
            .is_some_and(|bytes| bytes != b"null")
    }
}

impl PumpStore for FlakyStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.inner.load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        if record == PumpRecord::Checkpoint && bytes == b"null" {
            self.invalidations.fetch_add(1, Ordering::Relaxed);
        }

        if self.refusing(record) {
            return Err("flash refused the write");
        }

        self.inner.store(record, bytes)
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        if record == PumpRecord::Checkpoint {
            self.invalidations.fetch_add(1, Ordering::Relaxed);
        }

        if self.refusing(record) {
            return Err("flash refused the erase");
        }

        self.inner.erase(record)
    }
}

#[derive(Clone, Default)]
struct StickyOutputs {
    pins: Arc<Mutex<[bool; 4]>>,
    refuse_off: Arc<Mutex<Option<PumpId>>>,
    writes: Arc<Mutex<Vec<(PumpId, bool, bool)>>>,
}

impl StickyOutputs {
    fn pin(&self, id: PumpId) -> bool {
        self.pins.lock().unwrap()[id.index()]
    }

    fn refuse_off(&self, id: Option<PumpId>) {
        *self.refuse_off.lock().unwrap() = id;
    }

    fn switched_on(&self, id: PumpId) -> bool {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .any(|&(pump, on, applied)| pump == id && on && applied)
    }

    fn failed_offs(&self, id: PumpId) -> usize {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .filter(|&&(pump, on, applied)| pump == id && !on && !applied)
            .count()
    }
}

impl PumpOutputs for StickyOutputs {
    fn set(&mut self, id: PumpId, on: bool) -> Result<(), &'static str> {
        let refused = !on && *self.refuse_off.lock().unwrap() == Some(id);
        self.writes.lock().unwrap().push((id, on, !refused));

        if refused {
            return Err("relay driver refused");
        }

        self.pins.lock().unwrap()[id.index()] = on;
        Ok(())
    }
}

type FlakyService = PumpService<Arc<FlakyStore>, Arc<TestClock>>;

struct Bench {
    service: Arc<FlakyService>,
    controller: PumpController<StickyOutputs, Arc<FlakyStore>, Arc<TestClock>>,
    outputs: StickyOutputs,
    store: Arc<FlakyStore>,
    clock: Arc<TestClock>,
    now: Instant,
}

impl Bench {
    fn on(store: Arc<FlakyStore>, clock: Arc<TestClock>) -> Self {
        let service = Arc::new(PumpService::new(
            PumpSettings::DEFAULT,
            Arc::clone(&store),
            Arc::clone(&clock),
            None,
        ));
        let outputs = StickyOutputs::default();
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

    fn new() -> Self {
        Self::on(Arc::default(), TestClock::synced(EPOCH))
    }

    fn reboot(&self) -> Self {
        Self::on(Arc::clone(&self.store), Arc::clone(&self.clock))
    }

    fn tick(&mut self, levels: &WaterLevelSnapshot) {
        self.controller.tick(Some(levels), self.now);
    }

    fn run(&mut self, seconds: u64, levels: &WaterLevelSnapshot) {
        for _ in 0..seconds {
            self.now += Duration::from_secs(1);
            self.clock.advance(1);
            self.tick(levels);
        }
    }
}

const QUICK_DRAIN: WaterChangePlan = WaterChangePlan {
    drain_s: 1,
    settle_s: 0,
    refill_s: 4,
};

#[test]
fn each_water_mover_needs_a_successful_off_for_its_opposite_even_from_idle() {
    for (starting, opposite) in [
        (PumpId::Drain, PumpId::Makeup),
        (PumpId::Makeup, PumpId::Drain),
    ] {
        let mut bench = Bench::new();
        let levels = healthy();
        bench.tick(&levels);
        bench.outputs.refuse_off(Some(opposite));
        if starting == PumpId::Drain {
            bench.service.start_water_change(PLAN).unwrap();
        } else {
            bench.service.start_top_off(60).unwrap();
        }
        bench.tick(&levels);
        assert!(!bench.outputs.switched_on(starting));
        assert!(bench.outputs.failed_offs(opposite) > 0);

        bench.outputs.refuse_off(None);
        bench.run(1, &levels);
        assert!(bench.outputs.pin(starting));
    }
}

#[test]
fn regression_a_stalled_drain_releases_circulation() {
    let mut bench = Bench::new();
    let levels = healthy();
    bench.service.set_supervision(Supervision::Active);
    bench.tick(&levels);
    assert!(bench.outputs.pin(PumpId::Return));

    bench.service.start_water_change(PLAN).expect("accepted");
    bench.outputs.refuse_off(Some(PumpId::Makeup));
    bench.run(60 * 60, &levels);

    assert!(bench.outputs.failed_offs(PumpId::Makeup) > 0);
    assert!(!bench.outputs.switched_on(PumpId::Drain));
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
    let status = bench.service.status();
    assert_eq!(
        status.operation.state,
        OperationState::Idle,
        "an hour elapsed without the drain ever switching on"
    );
    let record = status.last_water_change.expect("stalled drain recorded");
    assert_eq!(record.drain_ran_s, 0);
    assert!(!matches!(
        record.result,
        OperationResult::Completed | OperationResult::SumpFull | OperationResult::Overflow
    ));
    assert!(bench.outputs.pin(PumpId::Return));
    assert!(bench.outputs.pin(PumpId::Uv));
}

#[test]
fn the_drain_waits_for_a_successful_return_off() {
    let mut bench = Bench::new();
    let levels = healthy();
    bench.service.set_supervision(Supervision::Active);
    bench.tick(&levels);
    assert!(bench.outputs.pin(PumpId::Return));

    bench.outputs.refuse_off(Some(PumpId::Return));
    bench.service.start_water_change(PLAN).expect("accepted");
    bench.run(5, &levels);

    assert!(!bench.outputs.switched_on(PumpId::Drain));
    let status = bench.service.status();
    assert_eq!(status.channels.drain.blocked_by, Some(Blocker::Interlock));

    bench.outputs.refuse_off(None);
    bench.run(2, &levels);
    assert!(!bench.outputs.pin(PumpId::Return));
    assert!(bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_drain_that_fails_to_switch_off_keeps_the_makeup_off() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_water_change(QUICK_DRAIN).expect("idle");
    bench.tick(&healthy);
    assert!(bench.outputs.pin(PumpId::Drain));

    bench.outputs.refuse_off(Some(PumpId::Drain));
    bench.run(5, &healthy);

    let status = bench.service.status();
    assert_eq!(status.operation.state, OperationState::Refilling);
    assert_eq!(status.channels.drain.fault, Some(ChannelFault::OutputError));
    assert!(status.channels.drain.on, "the drain may still be running");
    assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::Interlock));
    assert!(bench.outputs.pin(PumpId::Drain));
    assert!(!bench.outputs.pin(PumpId::Makeup));
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
    assert!(bench.outputs.failed_offs(PumpId::Drain) >= 4);

    bench.outputs.refuse_off(None);
    bench.run(1, &healthy);
    assert!(!bench.outputs.pin(PumpId::Drain));
    assert!(bench.outputs.pin(PumpId::Makeup));
    assert_eq!(bench.service.status().channels.drain.fault, None);
}

#[test]
fn a_water_change_does_not_drain_while_the_top_off_makeup_may_still_run() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_top_off(60).expect("idle");
    bench.run(1, &healthy);
    assert!(bench.outputs.pin(PumpId::Makeup));

    bench.outputs.refuse_off(Some(PumpId::Makeup));
    bench.service.start_water_change(PLAN).expect("preempts");
    bench.run(3, &healthy);

    let status = bench.service.status();
    assert_eq!(status.operation.state, OperationState::Draining);
    assert_eq!(status.operation.drain_ran_s, 0);
    assert_eq!(
        status.channels.makeup.fault,
        Some(ChannelFault::OutputError)
    );
    assert_eq!(status.channels.drain.blocked_by, Some(Blocker::Interlock));
    assert!(bench.outputs.pin(PumpId::Makeup));
    assert!(!bench.outputs.pin(PumpId::Drain));
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn an_api_start_that_cannot_be_recorded_switches_nothing_on() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.store.refuse(true);

    assert!(matches!(
        bench.service.start_water_change(PLAN),
        Err(StartError::NotPersisted(_))
    ));
    assert!(matches!(
        bench.service.start_top_off(60),
        Err(StartError::NotPersisted(_))
    ));
    bench.run(3, &healthy);

    assert_eq!(bench.service.status().operation, OperationStatus::IDLE);
    assert!(!bench.outputs.switched_on(PumpId::Drain));
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
}

#[test]
fn a_water_change_that_cannot_be_recorded_leaves_the_running_top_off_alone() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_top_off(60).expect("idle");
    bench.run(1, &healthy);
    bench.store.refuse(true);

    assert!(matches!(
        bench.service.start_water_change(PLAN),
        Err(StartError::NotPersisted(_))
    ));
    bench.run(1, &healthy);

    let status = bench.service.status();
    assert_eq!(status.operation.kind, Some(OperationKind::TopOff));
    assert_eq!(status.last_top_off, None);
    assert!(bench.outputs.pin(PumpId::Makeup));
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_scheduled_start_that_cannot_be_recorded_is_skipped_for_storage() {
    let mut bench = Bench::new();
    let healthy = healthy();
    schedule_top_off_on(&bench.service);
    bench.tick(&healthy);
    bench.store.refuse(true);

    bench.clock.set_civil(Some(sunday_ten_am()));
    bench.run(3, &healthy);

    let skipped = bench.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Storage);
    assert_eq!(skipped.blocked_by, None);
    assert!(!bench.outputs.switched_on(PumpId::Makeup));
}

fn seed_flaky_checkpoint(store: &FlakyStore, operation: Operation, checkpoint_at: Option<u64>) {
    store.inner.put(
        PumpRecord::Checkpoint,
        serde_json::to_vec(&Checkpoint {
            operation,
            checkpoint_at,
        })
        .expect("encode"),
    );
}

#[test]
fn an_automatic_resume_that_cannot_be_recorded_suspends_without_switching() {
    let store = Arc::new(FlakyStore::default());
    seed_flaky_checkpoint(&store, interrupted_drain(EPOCH - 200), Some(EPOCH - 100));
    store.refuse(true);
    let mut bench = Bench::on(store, TestClock::synced(EPOCH));
    let healthy = healthy();

    bench.tick(&healthy);
    bench.run(3, &healthy);

    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Suspended
    );
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn an_api_resume_that_cannot_be_recorded_stays_suspended() {
    let store = Arc::new(FlakyStore::default());
    seed_flaky_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut bench = Bench::on(store, TestClock::synced(EPOCH));
    let healthy = healthy();
    bench.tick(&healthy);
    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Suspended
    );

    bench.store.refuse(true);
    assert!(matches!(
        bench.service.resume(OperationKind::WaterChange),
        Err(StartError::NotPersisted(_))
    ));
    bench.run(3, &healthy);

    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Suspended
    );
    assert!(!bench.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_cancel_that_cannot_be_recorded_stops_the_drain_and_retries_until_it_lands() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_water_change(PLAN).expect("idle");
    bench.run(1, &healthy);
    assert!(bench.outputs.pin(PumpId::Drain));

    bench.store.refuse(true);
    assert!(matches!(
        bench.service.cancel(OperationKind::WaterChange),
        Err(CancelError::NotPersisted(_))
    ));
    bench.run(1, &healthy);
    assert!(!bench.outputs.pin(PumpId::Drain));
    assert!(bench.service.status().invalidation_pending);
    assert!(bench.store.holds_checkpoint());

    let tried = bench.store.invalidations.load(Ordering::Relaxed);
    bench.run(3, &healthy);
    assert!(bench.store.invalidations.load(Ordering::Relaxed) >= tried + 3);
    assert!(bench.service.status().invalidation_pending);

    bench.store.refuse(false);
    bench.run(1, &healthy);
    assert!(!bench.service.status().invalidation_pending);
    assert!(!bench.store.holds_checkpoint());

    let mut rebooted = bench.reboot();
    rebooted.tick(&healthy);
    rebooted.run(5, &healthy);
    assert_eq!(rebooted.service.status().operation, OperationStatus::IDLE);
    assert!(!rebooted.outputs.switched_on(PumpId::Drain));
}

#[test]
fn a_new_operation_supersedes_a_pending_invalidation() {
    let mut bench = Bench::new();
    let healthy = healthy();
    bench.tick(&healthy);
    bench.service.start_top_off(60).expect("idle");
    bench.run(1, &healthy);
    bench.store.refuse(true);
    assert!(bench.service.cancel(OperationKind::TopOff).is_err());
    bench.run(1, &healthy);

    bench.store.refuse(false);
    bench.service.start_top_off(60).expect("idle");
    assert!(!bench.service.status().invalidation_pending);
    bench.run(1, &healthy);
    assert!(bench.store.holds_checkpoint());
    assert_eq!(
        bench.service.status().operation.kind,
        Some(OperationKind::TopOff)
    );
}

fn unplugged() -> WaterLevelSnapshot {
    WaterLevelSnapshot::dry()
}

#[test]
fn an_unplugged_harness_mid_drain_holds_the_return_off() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));

    rig.run(1, Some(&unplugged()));
    let status = rig.service.status();
    assert!(status.level_sensors_suspect);
    assert!(status.return_held_after_drain);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::LevelsUnavailableAfterDrain)
    );
    assert_eq!(status.channels.uv.blocked_by, Some(Blocker::ReturnOff));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(3600, Some(&unplugged()));
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&running_sump()));
    assert!(!rig.service.status().return_held_after_drain);
}

fn abort_a_water_change(rig: &mut Rig) {
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
}

#[test]
fn an_unplugged_harness_within_thirty_minutes_of_an_aborted_drain_holds_the_return() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(29 * 60, Some(&running_sump()));

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().return_held_after_drain);
    assert!(!rig.pins()[PumpId::Return.index()]);
}

#[test]
fn an_unplugged_harness_long_after_an_aborted_drain_keeps_the_return() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(31 * 60, Some(&running_sump()));

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().level_sensors_suspect);
    assert!(!rig.service.status().return_held_after_drain);
    assert_eq!(rig.pins(), [true, true, false, false]);
}

#[test]
fn an_unplugged_harness_after_a_water_change_that_filled_the_sump_keeps_the_return() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(6, Some(&healthy()));
    rig.run(3, Some(&full_sump()));
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::SumpFull)
    );

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().level_sensors_suspect);
    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);
}

#[test]
fn an_unplugged_harness_while_a_water_change_is_suspended_holds_the_return() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.run(5, Some(&running_sump()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    rig.run(1, Some(&unplugged()));

    assert!(rig.service.status().return_held_after_drain);
    assert_eq!(rig.pins(), [false, false, false, false]);
}

#[test]
fn unavailable_supervision_refuses_new_water_movers() {
    let mut rig = Rig::new();
    rig.run(5, Some(&healthy()));
    assert_eq!(rig.service.status().supervision, Supervision::Off);

    rig.service.set_supervision(Supervision::Unavailable);
    let status = rig.service.status();
    assert_eq!(status.supervision, Supervision::Unavailable);
    assert_eq!(
        rig.service.pump_telemetry().supervision,
        Supervision::Unavailable
    );
    assert_eq!(
        rig.service.start_top_off(60),
        Err(StartError::Blocked(Blocker::SupervisionUnavailable))
    );
    assert_eq!(
        rig.service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::SupervisionUnavailable))
    );
    rig.run(2, Some(&healthy()));
    assert_eq!(rig.pins(), [true, true, false, false]);

    rig.service.set_supervision(Supervision::Active);
    assert_eq!(rig.service.start_top_off(60), Ok(()));
}

#[test]
fn regression_losing_supervision_while_settling_keeps_makeup_off() {
    let mut bench = Bench::new();
    let levels = healthy();
    bench.service.set_supervision(Supervision::Active);
    bench.tick(&levels);
    bench.service.start_water_change(PLAN).expect("accepted");
    bench.tick(&levels);
    bench.run(u64::from(PLAN.drain_s), &levels);
    assert_eq!(
        bench.service.status().operation.state,
        OperationState::Settling
    );
    assert!(!bench.outputs.switched_on(PumpId::Makeup));

    bench.service.set_supervision(Supervision::Unavailable);
    bench.run(u64::from(PLAN.settle_s + PLAN.refill_s), &levels);

    assert!(
        !bench.outputs.switched_on(PumpId::Makeup),
        "makeup switched on after watchdog supervision was lost"
    );
}

#[test]
fn a_scheduled_window_without_supervision_is_skipped_with_that_reason() {
    let mut rig = Rig::new();
    let healthy = healthy();
    schedule_two_minutes(&rig.service);
    rig.tick(Some(&healthy));
    rig.service.set_supervision(Supervision::Unavailable);

    assert!(!fire_at(&mut rig, EPOCH, 0, &healthy));

    let skipped = rig.service.status().last_skipped.expect("skipped");
    assert_eq!(skipped.reason, SkipReason::Blocked);
    assert_eq!(skipped.blocked_by, Some(Blocker::SupervisionUnavailable));
}

#[test]
fn a_suspended_operation_does_not_resume_without_supervision() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.run(5, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );

    rig.service.set_supervision(Supervision::Unavailable);
    assert_eq!(
        rig.service.resume(OperationKind::WaterChange),
        Err(StartError::Blocked(Blocker::SupervisionUnavailable))
    );

    rig.service.set_supervision(Supervision::Active);
    assert_eq!(rig.service.resume(OperationKind::WaterChange), Ok(()));
}

#[test]
fn an_interrupted_drain_waits_for_supervision_before_resuming_on_its_own() {
    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 20), Some(EPOCH - 10));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.service.set_supervision(Supervision::Unavailable);

    rig.run(3, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );
    assert!(!rig.pins()[PumpId::Drain.index()]);

    rig.service.set_supervision(Supervision::Active);
    rig.run(2, Some(&healthy()));
    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Draining
    );
}

#[test]
fn pump_telemetry_carries_the_storage_and_hold_flags() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));
    rig.run(1, Some(&unplugged()));

    let telemetry = rig.service.pump_telemetry();
    assert!(telemetry.return_held_after_drain);
    assert!(!telemetry.invalidation_pending);
}

#[test]
fn levels_lost_mid_drain_hold_the_return_until_fresh_data_returns() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));
    rig.service.start_water_change(PLAN).expect("idle");
    rig.run(1, Some(&running_sump()));

    rig.run(1, None);
    let status = rig.service.status();
    assert!(!status.level_sensors_suspect);
    assert!(status.return_held_after_drain);
    assert_eq!(
        status.channels.return_pump.blocked_by,
        Some(Blocker::LevelsUnavailableAfterDrain)
    );
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(3600, None);
    assert_eq!(rig.pins(), [false, false, false, false]);

    rig.run(1, Some(&running_sump()));
    assert!(!rig.service.status().return_held_after_drain);
}

#[test]
fn levels_lost_within_thirty_minutes_of_an_aborted_drain_hold_the_return() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(29 * 60, Some(&running_sump()));

    rig.run(1, None);

    assert!(rig.service.status().return_held_after_drain);
    assert!(!rig.pins()[PumpId::Return.index()]);
}

#[test]
fn levels_lost_at_rest_keep_the_return_running() {
    let mut rig = Rig::new();
    rig.run(5, Some(&running_sump()));

    rig.run(5, None);

    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);

    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(31 * 60, Some(&running_sump()));
    rig.run(1, None);
    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);
}

#[test]
fn a_finished_water_change_records_when_it_ended() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);

    let record = rig.service.status().last_water_change.expect("recorded");
    assert_eq!(record.finished_at, Some(EPOCH + 6));
    let stored: LastOperations = decode(&rig.store, PumpRecord::LastOperations).expect("stored");
    assert_eq!(stored.water_change, Some(record));
}

#[test]
fn the_blind_return_hold_window_survives_a_restart() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(10 * 60, Some(&running_sump()));

    let mut restarted = Rig::build(Arc::clone(&rig.store), Arc::clone(&rig.clock), None);
    restarted.run(1, None);
    assert!(restarted.service.status().return_held_after_drain);
    assert!(!restarted.pins()[PumpId::Return.index()]);

    restarted.run(1, Some(&running_sump()));
    assert!(!restarted.service.status().return_held_after_drain);

    rig.clock.advance(25 * 60);
    let mut later = Rig::build(Arc::clone(&rig.store), Arc::clone(&rig.clock), None);
    later.run(1, None);
    assert!(!later.service.status().return_held_after_drain);
    assert!(later.pins()[PumpId::Return.index()]);
}

#[test]
fn regression_a_recent_aborted_drain_keeps_the_return_off_before_clock_sync() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    let record = rig
        .service
        .status()
        .last_water_change
        .expect("aborted drain");
    assert!(record.finished_at.is_some());

    let mut restarted = Rig::build(rig.store, TestClock::unsynced(), None);
    restarted.run(1, None);
    assert!(
        !restarted.pins()[PumpId::Return.index()],
        "the return switched on before the clock or level readings recovered"
    );
    assert!(restarted.service.status().return_held_after_drain);
}

#[test]
fn regression_an_active_return_hold_survives_reboot_after_thirty_minutes() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);
    rig.run(31 * 60, None);
    assert!(rig.service.status().return_held_after_drain);
    assert!(!rig.pins()[PumpId::Return.index()]);

    let epoch = rig.clock.epoch_seconds().expect("synchronized clock");
    let mut restarted = Rig::build(rig.store, TestClock::synced(epoch), None);
    restarted.run(1, None);
    assert!(
        !restarted.pins()[PumpId::Return.index()],
        "reboot released the hold while the same sensor outage continued"
    );
    assert!(restarted.service.status().return_held_after_drain);

    restarted.run(31, Some(&running_sump()));
    assert!(!restarted.service.status().return_held_after_drain);
    assert!(restarted.pins()[PumpId::Return.index()]);
}

#[test]
fn the_return_hold_is_stored_when_set_and_when_released() {
    let mut rig = Rig::new();
    abort_a_water_change(&mut rig);

    rig.run(1, None);
    assert_eq!(
        decode::<bool>(&rig.store, PumpRecord::ReturnHold),
        Some(true)
    );

    rig.run(1, Some(&running_sump()));
    assert_eq!(
        decode::<bool>(&rig.store, PumpRecord::ReturnHold),
        Some(false)
    );
}

#[test]
fn storage_without_a_return_hold_starts_unheld() {
    let mut rig = Rig::new();

    rig.run(5, None);

    assert!(!rig.service.status().return_held_after_drain);
    assert!(rig.pins()[PumpId::Return.index()]);
    assert_eq!(decode::<bool>(&rig.store, PumpRecord::ReturnHold), None);
}

#[derive(Default)]
struct HoldRefusingStore {
    inner: VolatilePumpStore,
    refuse: std::sync::atomic::AtomicBool,
}

impl PumpStore for HoldRefusingStore {
    fn load(&self, record: PumpRecord) -> Option<Vec<u8>> {
        self.inner.load(record)
    }

    fn store(&self, record: PumpRecord, bytes: &[u8]) -> Result<(), &'static str> {
        if record == PumpRecord::ReturnHold && self.refuse.load(Ordering::Relaxed) {
            return Err("flash refused the write");
        }
        self.inner.store(record, bytes)
    }

    fn erase(&self, record: PumpRecord) -> Result<(), &'static str> {
        self.inner.erase(record)
    }
}

#[test]
fn a_refused_return_hold_write_is_retried() {
    let store = Arc::new(HoldRefusingStore::default());
    let clock = TestClock::synced(EPOCH);
    let service = Arc::new(PumpService::new(
        PumpSettings::DEFAULT,
        Arc::clone(&store),
        Arc::clone(&clock),
        None,
    ));
    let outputs = RecordingOutputs::default();
    let mut controller = PumpController::new(outputs.clone(), Arc::clone(&service), timing());
    let mut now = Instant::now();
    let mut run = |seconds: u64, levels: Option<&WaterLevelSnapshot>| {
        for _ in 0..seconds {
            now += Duration::from_secs(1);
            clock.advance(1);
            controller.tick(levels, now);
        }
    };

    run(5, Some(&running_sump()));
    service.start_water_change(PLAN).expect("idle");
    run(1, Some(&running_sump()));
    service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    store.refuse.store(true, Ordering::Relaxed);
    run(3, None);

    assert!(service.status().return_held_after_drain);
    assert!(!outputs.state()[PumpId::Return.index()]);
    assert!(store.inner.get(PumpRecord::ReturnHold).is_none());

    store.refuse.store(false, Ordering::Relaxed);
    run(1, None);

    assert_eq!(
        store.inner.get(PumpRecord::ReturnHold).as_deref(),
        Some(&b"true"[..])
    );
}

#[test]
fn a_history_without_an_end_time_still_loads() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::LastOperations,
        br#"{"top_off":{"kind":"top_off","started_at":5,"drain_ran_s":0,"settle_ran_s":0,"refill_ran_s":30,"result":"completed","resumed":false},"water_change":null}"#.to_vec(),
    );
    let service = service_on(&store, TestClock::synced(EPOCH));

    let record = service.status().last_top_off.expect("loaded");
    assert_eq!(record.refill_ran_s, 30);
    assert_eq!(record.finished_at, None);
}
