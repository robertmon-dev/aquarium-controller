use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{watch, Notify};
use tokio::task::LocalSet;
use tokio::time::Instant;

use crate::adapters::outbound::memory::NoLeakSensor;
use crate::application::reservoir::fixtures::*;
use crate::application::reservoir::{
    reservoir_loop, ReservoirController, ReservoirService, ReservoirTiming,
};
use crate::domain::pump::Supervision;
use crate::domain::reservoir::{
    FillPhase, FillResult, OverrideTarget, ReservoirBlocker, ReservoirChannel, ReservoirSettings,
};
use crate::ports::inbound::ReservoirControl;
use crate::ports::outbound::{LevelSample, TdsSample, WaterChangeActivity};

const TIMING: ReservoirTiming = ReservoirTiming {
    level_stale: Duration::from_secs(3),
    ..ReservoirTiming::DEFAULT
};

struct NoWaterChange;

impl WaterChangeActivity for NoWaterChange {
    fn water_change_active(&self) -> bool {
        false
    }
}

struct Loop {
    service: Arc<Service>,
    relays: Relays,
    levels: watch::Sender<Option<LevelSample>>,
    tds: watch::Sender<Option<TdsSample>>,
    republish: Arc<Notify>,
}

fn spawn(settings: ReservoirSettings, timing: ReservoirTiming) -> Loop {
    let service = Arc::new(ReservoirService::new(
        settings,
        Arc::<FlakyStore>::default(),
        TestClock::synced(),
        None,
        timing,
    ));
    service.set_supervision(Supervision::Active);
    let relays = Relays::default();
    let (levels, level_samples) = watch::channel(None);
    let (tds, tds_samples) = watch::channel(None);
    let republish = Arc::new(Notify::new());

    tokio::task::spawn_local(reservoir_loop(
        ReservoirController::new(relays.clone(), Arc::clone(&service), timing),
        level_samples,
        tds_samples,
        NoLeakSensor,
        NoWaterChange,
        service.change_signal(),
        Arc::clone(&republish),
        timing,
        Arc::new(AtomicU32::new(0)),
    ));

    Loop {
        service,
        relays,
        levels,
        tds,
        republish,
    }
}

fn publish_levels(levels: &watch::Sender<Option<LevelSample>>) {
    levels.send_replace(Some(LevelSample {
        levels: barrel(&HALF),
        measured_at: Instant::now(),
    }));
}

fn publish_tds(tds: &watch::Sender<Option<TdsSample>>, ppm: f32) {
    tds.send_replace(Some(TdsSample {
        ppm: Some(ppm),
        measured_at: Instant::now(),
    }));
}

async fn mixer_on(service: &Service, relays: &Relays) {
    service
        .set_override(OverrideTarget::Mixer, true, Some(60))
        .expect("mixer on");
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert!(relays.pin(ReservoirChannel::Mixer));
}

#[tokio::test(start_paused = true)]
async fn the_loop_treats_quiet_level_sensors_as_unavailable() {
    LocalSet::new()
        .run_until(async {
            let rig = spawn(RO_ONLY, TIMING);
            publish_levels(&rig.levels);
            tokio::time::sleep(Duration::from_millis(10)).await;
            mixer_on(&rig.service, &rig.relays).await;

            tokio::time::sleep(Duration::from_millis(3500)).await;

            assert_eq!(rig.relays.pins(), [false; 4]);
            assert_eq!(
                rig.service.status().channels.mixer.blocked_by,
                Some(ReservoirBlocker::LevelsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_closed_level_channel_switches_everything_off_at_once() {
    LocalSet::new()
        .run_until(async {
            let Loop {
                service,
                relays,
                levels,
                tds: _tds,
                republish: _republish,
            } = spawn(RO_ONLY, TIMING);
            publish_levels(&levels);
            tokio::time::sleep(Duration::from_millis(10)).await;
            mixer_on(&service, &relays).await;

            drop(levels);
            tokio::time::sleep(Duration::from_millis(10)).await;

            assert_eq!(relays.pins(), [false; 4]);
            assert_eq!(
                service.status().channels.mixer.blocked_by,
                Some(ReservoirBlocker::LevelsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_tds_channel_closed_while_measuring_ends_the_fill_without_tds() {
    LocalSet::new()
        .run_until(async {
            let Loop {
                service,
                relays: _relays,
                levels,
                tds,
                republish: _republish,
            } = spawn(TARGETED, TIMING);
            let phase = || service.status().fill.map(|fill| fill.phase);
            publish_levels(&levels);
            publish_tds(&tds, 150.0);
            tokio::time::sleep(Duration::from_millis(10)).await;
            service.start_fill().expect("started");

            for _ in 0..50 {
                publish_levels(&levels);
                publish_tds(&tds, 150.0);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            for _ in 0..30 {
                if phase() == Some(FillPhase::Measuring) {
                    break;
                }
                publish_levels(&levels);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            assert_eq!(phase(), Some(FillPhase::Measuring));

            drop(tds);
            for _ in 0..40 {
                publish_levels(&levels);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            assert_eq!(
                service.status().last.map(|record| record.result),
                Some(FillResult::TdsUnavailable)
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_api_change_is_applied_without_waiting_for_the_ticker() {
    let slow = ReservoirTiming {
        tick: Duration::from_secs(60),
        level_stale: Duration::from_secs(600),
        ..ReservoirTiming::DEFAULT
    };
    LocalSet::new()
        .run_until(async {
            let rig = spawn(RO_ONLY, slow);
            publish_levels(&rig.levels);
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = tokio::time::timeout(Duration::from_millis(1), rig.republish.notified()).await;

            mixer_on(&rig.service, &rig.relays).await;

            tokio::time::timeout(Duration::from_millis(10), rig.republish.notified())
                .await
                .expect("republished");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn override_expiry_closes_the_valve_between_ticks() {
    LocalSet::new()
        .run_until(async {
            let rig = spawn(
                RO_ONLY,
                ReservoirTiming {
                    tick: Duration::from_secs(60),
                    level_stale: Duration::from_secs(60),
                    ..TIMING
                },
            );
            publish_levels(&rig.levels);
            tokio::time::sleep(Duration::from_millis(10)).await;
            rig.service
                .set_override(OverrideTarget::Tap, true, Some(1))
                .expect("open tap");
            tokio::time::sleep(Duration::from_millis(10)).await;
            assert!(rig.relays.pin(ReservoirChannel::TapValve));
            tokio::time::sleep(Duration::from_secs(1)).await;
            assert!(!rig.relays.pin(ReservoirChannel::TapValve));
        })
        .await;
}
