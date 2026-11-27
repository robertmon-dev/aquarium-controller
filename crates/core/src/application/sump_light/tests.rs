use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::LocalSet;

use super::{sump_light_loop, SumpLightService};
use crate::ports::inbound::SumpLightControl;
use crate::ports::outbound::SumpLightOutputs;

#[derive(Clone, Default)]
struct Pin {
    writes: Arc<Mutex<Vec<bool>>>,
    failing: Arc<Mutex<bool>>,
}

impl Pin {
    fn writes(&self) -> Vec<bool> {
        self.writes.lock().unwrap().clone()
    }

    fn fail(&self, failing: bool) {
        *self.failing.lock().unwrap() = failing;
    }
}

impl SumpLightOutputs for Pin {
    fn set(&mut self, on: bool) -> Result<(), &'static str> {
        if *self.failing.lock().unwrap() {
            return Err("refused");
        }
        self.writes.lock().unwrap().push(on);
        Ok(())
    }
}

async fn settle() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

fn spawn(service: &Arc<SumpLightService>, pin: &Pin, republish: &Arc<Notify>) {
    tokio::task::spawn_local(sump_light_loop(
        Arc::clone(service),
        pin.clone(),
        Arc::clone(republish),
    ));
}

#[test]
fn status_is_unavailable_without_the_loop() {
    let service = SumpLightService::new();
    assert!(!service.status().available);
}

#[test]
fn an_invalid_request_is_rejected() {
    let service = SumpLightService::new();
    assert!(service.set(false, Some(60)).is_err());
    assert!(service.set(true, Some(10)).is_err());
}

#[tokio::test(start_paused = true)]
async fn the_loop_drives_the_light_off_at_start() {
    LocalSet::new()
        .run_until(async {
            let service = Arc::new(SumpLightService::new());
            let pin = Pin::default();
            spawn(&service, &pin, &Arc::new(Notify::new()));
            settle().await;

            assert_eq!(pin.writes(), vec![false]);
            let status = service.status();
            assert!(status.available);
            assert!(!status.on);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn switching_on_drives_the_output_and_republishes() {
    LocalSet::new()
        .run_until(async {
            let service = Arc::new(SumpLightService::new());
            let pin = Pin::default();
            let republish = Arc::new(Notify::new());
            spawn(&service, &pin, &republish);
            settle().await;
            tokio::time::timeout(Duration::from_millis(1), republish.notified())
                .await
                .expect("the start-up state is published");

            let notified = republish.notified();
            service.set(true, None).unwrap();
            settle().await;

            assert_eq!(pin.writes(), vec![false, true]);
            assert!(service.status().on);
            tokio::time::timeout(Duration::from_millis(1), notified)
                .await
                .expect("telemetry republished");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_timed_request_switches_off_on_time() {
    LocalSet::new()
        .run_until(async {
            let service = Arc::new(SumpLightService::new());
            let pin = Pin::default();
            spawn(&service, &pin, &Arc::new(Notify::new()));
            settle().await;

            let status = service.set(true, Some(1800)).unwrap();
            assert_eq!(status.expires_in_s, Some(1800));
            settle().await;

            tokio::time::sleep(Duration::from_secs(1800)).await;
            settle().await;

            assert_eq!(pin.writes(), vec![false, true, false]);
            assert!(!service.status().on);
            assert_eq!(service.status().expires_in_s, None);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_write_is_retried_every_five_seconds() {
    LocalSet::new()
        .run_until(async {
            let service = Arc::new(SumpLightService::new());
            let pin = Pin::default();
            spawn(&service, &pin, &Arc::new(Notify::new()));
            settle().await;

            pin.fail(true);
            service.set(true, None).unwrap();
            settle().await;
            assert!(
                !service.status().on,
                "status shows what the output really does"
            );

            pin.fail(false);
            tokio::time::sleep(Duration::from_secs(5)).await;
            settle().await;

            assert_eq!(pin.writes(), vec![false, true]);
            assert!(service.status().on);
        })
        .await;
}
