use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::application::pump::{PumpController, PumpService};
use crate::domain::pump::{
    ChannelMode, Checkpoint, OperationKind, OperationStatus, OverrideError, Phase, PumpId,
    PumpSettings, RuntimeCounters, StoredOverride, WaterChangePlan,
};
use crate::domain::water::WaterLevelRole;
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::{PumpOutputs, PumpRecord, PumpStore};

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
