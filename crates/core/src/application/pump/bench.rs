use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::time::Instant;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::application::pump::{PumpController, PumpService};
use crate::domain::pump::{PumpId, PumpSettings};
use crate::domain::water::WaterLevelSnapshot;
use crate::ports::outbound::{PumpOutputs, PumpRecord, PumpStore};

#[derive(Default)]
pub(super) struct FlakyStore {
    pub(super) inner: VolatilePumpStore,
    refuse: std::sync::atomic::AtomicBool,
    pub(super) invalidations: AtomicU32,
}

impl FlakyStore {
    pub(super) fn refuse(&self, refuse: bool) {
        self.refuse.store(refuse, Ordering::Relaxed);
    }

    fn refusing(&self, record: PumpRecord) -> bool {
        matches!(record, PumpRecord::Checkpoint | PumpRecord::Flood)
            && self.refuse.load(Ordering::Relaxed)
    }

    pub(super) fn holds_checkpoint(&self) -> bool {
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
pub(super) struct StickyOutputs {
    pins: Arc<Mutex<[bool; 4]>>,
    refuse_off: Arc<Mutex<Option<PumpId>>>,
    writes: Arc<Mutex<Vec<(PumpId, bool, bool)>>>,
}

impl StickyOutputs {
    pub(super) fn pin(&self, id: PumpId) -> bool {
        self.pins.lock().unwrap()[id.index()]
    }

    pub(super) fn refuse_off(&self, id: Option<PumpId>) {
        *self.refuse_off.lock().unwrap() = id;
    }

    pub(super) fn switched_on(&self, id: PumpId) -> bool {
        self.writes
            .lock()
            .unwrap()
            .iter()
            .any(|&(pump, on, applied)| pump == id && on && applied)
    }

    pub(super) fn failed_offs(&self, id: PumpId) -> usize {
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

pub(super) type FlakyService = PumpService<Arc<FlakyStore>, Arc<TestClock>>;

pub(super) struct Bench {
    pub(super) service: Arc<FlakyService>,
    controller: PumpController<StickyOutputs, Arc<FlakyStore>, Arc<TestClock>>,
    pub(super) outputs: StickyOutputs,
    pub(super) store: Arc<FlakyStore>,
    pub(super) clock: Arc<TestClock>,
    now: Instant,
}

impl Bench {
    pub(super) fn on(store: Arc<FlakyStore>, clock: Arc<TestClock>) -> Self {
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

    pub(super) fn new() -> Self {
        Self::on(Arc::default(), TestClock::synced(EPOCH))
    }

    pub(super) fn reboot(&self) -> Self {
        Self::on(Arc::clone(&self.store), Arc::clone(&self.clock))
    }

    pub(super) fn tick(&mut self, levels: &WaterLevelSnapshot) {
        self.controller.tick(Some(levels), self.now);
    }

    pub(super) fn run(&mut self, seconds: u64, levels: &WaterLevelSnapshot) {
        for _ in 0..seconds {
            self.now += Duration::from_secs(1);
            self.clock.advance(1);
            self.tick(levels);
        }
    }
}
