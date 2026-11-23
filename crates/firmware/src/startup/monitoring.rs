use aquarium_core::application::{PumpService, ReservoirService};
use aquarium_core::domain::pump::Supervision;
use aquarium_core::ports::outbound::{Clock, PumpStore, ReservoirStore};
use esp_idf_hal::task::watchdog::{TWDTConfig, TWDTDriver, TWDT};
use esp_idf_svc::sys::{self, EspError};
use std::future::Future;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::time::MissedTickBehavior;
use tracing::{error, info, warn};

const HEALTH_INTERVAL: Duration = Duration::from_secs(30);
const NVS_ALARM_INTERVAL: Duration = Duration::from_secs(30);

const WATCHDOG_TIMEOUT: Duration = Duration::from_secs(10);
const WATCHDOG_CHECK: Duration = Duration::from_secs(2);

fn log_health() {
    unsafe {
        let stack_free = sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut());
        let internal_free = sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL);
        let internal_min_free = sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_INTERNAL);

        if stack_free < 16384 {
            error!(stack_free, "main task stack headroom is low");
        }

        info!(stack_free, internal_free, internal_min_free, "health");
    }
}

pub(super) async fn health_watch() {
    let mut ticker = tokio::time::interval(HEALTH_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        log_health();
    }
}

pub(super) async fn nvs_unavailable_alarm(error: EspError) {
    let mut ticker = tokio::time::interval(NVS_ALARM_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        error!(
            %error,
            "NVS partition unusable and left untouched: defaults in use, nothing is saved, \
             pump and barrel automation stopped; erase it deliberately as described in the README"
        );
    }
}

pub(super) trait Supervised {
    fn set_supervision(&self, supervision: Supervision);
}

impl<S: PumpStore, C: Clock> Supervised for PumpService<S, C> {
    fn set_supervision(&self, supervision: Supervision) {
        PumpService::set_supervision(self, supervision);
    }
}

impl<S: ReservoirStore, C: Clock> Supervised for ReservoirService<S, C> {
    fn set_supervision(&self, supervision: Supervision) {
        ReservoirService::set_supervision(self, supervision);
    }
}

fn supervise(services: &[Arc<dyn Supervised>], supervision: Supervision) {
    for service in services {
        service.set_supervision(supervision);
    }
}

pub(super) fn watchdog(
    twdt: TWDT<'static>,
    heartbeats: Vec<Arc<AtomicU32>>,
    supervised: Vec<Arc<dyn Supervised>>,
) -> Option<impl Future<Output = ()>> {
    if heartbeats.is_empty() {
        return None;
    }
    supervise(&supervised, Supervision::Unavailable);
    let config = TWDTConfig {
        duration: WATCHDOG_TIMEOUT,
        panic_on_trigger: true,
        subscribed_idle_tasks: Default::default(),
    };

    match TWDTDriver::new(twdt, &config) {
        Ok(driver) => Some(main_watchdog(driver, heartbeats, supervised)),
        Err(error) => {
            error!(%error, "Watchdog unavailable, pump and barrel supervision unavailable");
            None
        }
    }
}

async fn main_watchdog(
    mut driver: TWDTDriver<'static>,
    heartbeats: Vec<Arc<AtomicU32>>,
    supervised: Vec<Arc<dyn Supervised>>,
) {
    let mut subscription = match driver.watch_current_task() {
        Ok(subscription) => subscription,
        Err(error) => {
            error!(%error, "Watchdog could not watch the main task, pump and barrel supervision unavailable");
            return;
        }
    };

    supervise(&supervised, Supervision::Active);
    info!(
        timeout_s = WATCHDOG_TIMEOUT.as_secs(),
        loops = heartbeats.len(),
        "Watchdog armed"
    );

    let mut ticker =
        tokio::time::interval_at(tokio::time::Instant::now() + WATCHDOG_CHECK, WATCHDOG_CHECK);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut last_beats: Vec<u32> = heartbeats
        .iter()
        .map(|beat| beat.load(Ordering::Relaxed))
        .collect();

    loop {
        ticker.tick().await;
        let beats: Vec<u32> = heartbeats
            .iter()
            .map(|beat| beat.load(Ordering::Relaxed))
            .collect();

        if beats
            .iter()
            .zip(&last_beats)
            .any(|(beat, last)| beat == last)
        {
            supervise(&supervised, Supervision::Unavailable);
            warn!("A control loop heartbeat stalled, watchdog not fed");
            continue;
        }

        last_beats = beats;

        match subscription.feed() {
            Ok(()) => supervise(&supervised, Supervision::Active),
            Err(error) => {
                error!(%error, "Watchdog feed failed");
                supervise(&supervised, Supervision::Unavailable);
            }
        }
    }
}
