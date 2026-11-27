use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde::de::DeserializeOwned;
use tokio::net::TcpListener;
use tokio::time::Instant;

use aquarium_core::adapters::inbound::http;
use aquarium_core::adapters::outbound::memory::{
    InMemoryTelemetryStore, VolatileCalibrationStore, VolatileReservoirStore,
};
use aquarium_core::application::reservoir::Observation;
use aquarium_core::application::{CalibrationService, ReservoirService, ReservoirTiming};
use aquarium_core::domain::pump::Supervision;
use aquarium_core::domain::reservoir::{ReservoirConditions, ReservoirSettings};
use aquarium_core::ports::inbound::ReservoirControl;
use aquarium_core::ports::outbound::{Clock, ReservoirRecord, ReservoirStore, TdsSample};

use super::{body, calibration_defaults, config, heaters, lighting, pumps, raw, FixedClock, TOKEN};

pub type TestReservoir = ReservoirService<VolatileReservoirStore, FixedClock>;

pub fn reservoir() -> Arc<TestReservoir> {
    Arc::new(ReservoirService::new(
        ReservoirSettings::DEFAULT,
        VolatileReservoirStore::default(),
        FixedClock,
        None,
        ReservoirTiming::DEFAULT,
    ))
}

pub const REFUSED_WRITE: &str = "the barrel store refused the write";

#[derive(Default)]
pub struct RefusingReservoirStore {
    pub inner: VolatileReservoirStore,
    pub refuse: AtomicBool,
}

impl RefusingReservoirStore {
    pub fn refuse(&self, refuse: bool) {
        self.refuse.store(refuse, Ordering::SeqCst);
    }

    fn check(&self) -> Result<(), &'static str> {
        if self.refuse.load(Ordering::SeqCst) {
            Err(REFUSED_WRITE)
        } else {
            Ok(())
        }
    }
}

impl ReservoirStore for RefusingReservoirStore {
    fn load(&self, record: ReservoirRecord) -> Result<Option<Vec<u8>>, &'static str> {
        self.inner.load(record)
    }

    fn store(&self, record: ReservoirRecord, bytes: &[u8]) -> Result<(), &'static str> {
        self.check()?;
        self.inner.store(record, bytes)
    }

    fn erase(&self, record: ReservoirRecord) -> Result<(), &'static str> {
        self.check()?;
        self.inner.erase(record)
    }
}

pub type RefusingReservoir = ReservoirService<Arc<RefusingReservoirStore>, FixedClock>;

pub fn refusing_reservoir() -> (Arc<RefusingReservoir>, Arc<RefusingReservoirStore>) {
    let store = Arc::new(RefusingReservoirStore::default());
    let reservoir = Arc::new(ReservoirService::new(
        ReservoirSettings::DEFAULT,
        Arc::clone(&store),
        FixedClock,
        None,
        ReservoirTiming::DEFAULT,
    ));
    (reservoir, store)
}

pub async fn start_with_reservoir<R: ReservoirControl + 'static>(reservoir: Arc<R>) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(
        listener,
        Arc::new(InMemoryTelemetryStore::new()),
        lighting(),
        Arc::new(CalibrationService::new(
            calibration_defaults(),
            VolatileCalibrationStore,
        )),
        heaters(),
        pumps(),
        reservoir,
        config(),
    ));

    addr
}

pub const RESERVOIR_FILL: &str = "/api/reservoir/fill";
pub const RESERVOIR_FAULT: &str = "/api/reservoir/fault";
pub const FAULT_LOCK: &str =
    "the barrel fill is locked by a fault; clear it with DELETE /api/reservoir/fault";

pub const HALF_BARREL: ReservoirConditions = ReservoirConditions {
    empty: false,
    ready: true,
    full: false,
    full_faulty: false,
    leak: None,
};

pub fn observe_barrel<S: ReservoirStore, C: Clock>(
    reservoir: &ReservoirService<S, C>,
    conditions: Option<ReservoirConditions>,
    water_change_active: bool,
) {
    let now = Instant::now();
    reservoir.observe(Observation {
        water_change_active,
        tds: Some(TdsSample {
            ppm: Some(150.0),
            measured_at: now,
        }),
        ..Observation::idle(conditions, now)
    });
}

pub fn ready_barrel() -> Arc<TestReservoir> {
    let reservoir = reservoir();
    reservoir.set_supervision(Supervision::Active);
    observe_barrel(&reservoir, Some(HALF_BARREL), false);
    reservoir
}

pub fn status_of(response: &str) -> u16 {
    response
        .get(9..12)
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status code in: {response}"))
}

pub fn assert_refused(response: &str, code: u16, reason: &str) {
    assert_eq!(status_of(response), code, "got: {response}");
    assert!(
        response.contains(reason),
        "expected {reason:?} in: {response}"
    );
}

pub fn parse_body<T: DeserializeOwned>(response: &str) -> T {
    let (_, payload) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no body in: {response}"));
    serde_json::from_str(payload.trim()).unwrap_or_else(|error| panic!("{error}: {payload}"))
}

pub async fn post_fill(addr: SocketAddr) -> String {
    raw(addr, &body("POST", RESERVOIR_FILL, Some(TOKEN), "")).await
}

pub async fn delete_at(addr: SocketAddr, path: &str) -> String {
    raw(addr, &body("DELETE", path, Some(TOKEN), "")).await
}
