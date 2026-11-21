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
    Blocker, ChannelFault, ChannelMode, Checkpoint, LevelConditions, Operation, OperationEvent,
    OperationKind, OperationPlan, OperationResult, OperationState, OperationStatus, OverrideError,
    Phase, PumpId, PumpSettings, PumpSettingsPatch, RuntimeCounters, SkipReason, SkippedWindow,
    StoredOverride, TopOffWindow, WaterChangePlan,
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
    PumpTiming {
        return_restart_delay: Duration::from_secs(30),
        level_stale: Duration::from_secs(1),
        tick: Duration::from_secs(1),
        checkpoint_interval: Duration::from_secs(10),
        runtime_flush: Duration::from_secs(600),
    }
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
    assert_eq!(service.start_water_change(PLAN), Ok(()));
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
    service.cancel(OperationKind::TopOff);

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

    service.cancel(OperationKind::TopOff);
    assert_eq!(
        service.status().operation.kind,
        Some(OperationKind::WaterChange)
    );

    service.cancel(OperationKind::WaterChange);
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
    operation.advance(4000, Some(HEALTHY));
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

    assert_eq!(rig.pins(), [true, true, false, false]);
    assert_eq!(
        rig.service
            .status()
            .last_water_change
            .map(|record| record.result),
        Some(OperationResult::Aborted)
    );
}

#[test]
fn a_failed_output_is_reported_as_a_fault() {
    let mut rig = Rig::new();
    *rig.outputs.failing.lock().unwrap() = Some(PumpId::Uv);

    rig.tick(Some(&healthy()));

    let uv = rig.service.status().channels.uv;
    assert_eq!((uv.on, uv.fault), (false, Some(ChannelFault::OutputError)));
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
fn a_running_operation_is_checkpointed_every_ten_seconds() {
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
    rig.run(10, Some(&healthy));

    let checkpoint: Checkpoint = decode(&rig.store, PumpRecord::Checkpoint).expect("saved");
    assert_eq!(checkpoint.operation.ran_ms[Phase::Draining.index()], 10_000);
    assert_eq!(checkpoint.checkpoint_at, Some(EPOCH + 10));
}

#[test]
fn a_timed_uv_override_expires_with_the_clock() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.service
        .set_override(PumpId::Uv, Some(60))
        .expect("valid");
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
    rig.service.cancel(OperationKind::WaterChange);

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

    rig.service.cancel(OperationKind::TopOff);
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
    let (service, outputs, republish, _heartbeat) = spawn_loop_with_heartbeat(levels);
    (service, outputs, republish)
}

fn spawn_loop_with_heartbeat(
    levels: &InMemoryWaterLevelStore,
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
        levels.subscribe(),
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
    service.cancel(OperationKind::TopOff);

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
    operation.advance(10_000, Some(HEALTHY));
    operation.advance(5000, Some(HEALTHY));
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
fn a_finished_operation_erases_its_checkpoint_before_recording_the_history() {
    let log = Log::default();
    let (service, _clock) = logged_service(&log);

    service.observe(Some(HEALTHY), Duration::ZERO, Instant::now());
    service.start_top_off(60).expect("idle");
    log.lock().unwrap().clear();
    service.cancel(OperationKind::TopOff);

    assert!(position(&log, "erase pump_op") < position(&log, "store pump_last"));
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
fn a_scheduled_start_switches_the_relay_before_writing_its_checkpoint() {
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

    assert!(position(&log, "set makeup on") < position(&log, "store pump_op"));
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

fn finish_a_water_change(service: &Service) {
    observe(service, Some(HEALTHY));
    service.start_water_change(PLAN).expect("idle");
    service.cancel(OperationKind::WaterChange);
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
    service.cancel(OperationKind::TopOff);

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
    service.cancel(OperationKind::WaterChange);

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
    rig.service.cancel(OperationKind::WaterChange);
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
            let (_service, _outputs, _republish, heartbeat) = spawn_loop_with_heartbeat(&levels);

            tokio::time::sleep(Duration::from_millis(3500)).await;

            assert!(
                heartbeat.load(Ordering::Relaxed) >= 4,
                "{}",
                heartbeat.load(Ordering::Relaxed)
            );
        })
        .await;
}
