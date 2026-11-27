use std::sync::{Arc, Mutex};
use std::time::Duration;

use tokio::sync::Notify;
use tokio::task::LocalSet;
use tokio::time::timeout;

use super::{lighting_loop, LightingService};
use crate::adapters::outbound::memory::VolatileLightingStore;
use crate::domain::cron::CivilTime;
use crate::domain::lighting::{
    ChannelLevels, LightingPalette, LightingPalettePatch, LightingTarget,
};
use crate::domain::metrics::LightingProfile;
use crate::ports::inbound::{LightingControl, LightingPaletteError};
use crate::ports::outbound::{Clock, LightingOutputs, LightingPersistence, LightingStatusSource};

struct NoonInJune;

impl Clock for NoonInJune {
    fn epoch_seconds(&self) -> Option<u64> {
        Some(1_750_000_000)
    }

    fn uptime_seconds(&self) -> u64 {
        0
    }

    fn local_minute_of_day(&self) -> Option<u16> {
        Some(12 * 60)
    }

    fn local_month(&self) -> Option<u8> {
        Some(6)
    }

    fn local_civil_time(&self) -> Option<CivilTime> {
        None
    }
}

#[derive(Clone, Default)]
struct RecordingOutputs {
    written: Arc<Mutex<Vec<ChannelLevels>>>,
    failing: Arc<Mutex<bool>>,
}

impl RecordingOutputs {
    fn written(&self) -> Vec<ChannelLevels> {
        self.written.lock().unwrap().clone()
    }

    fn fail(&self, failing: bool) {
        *self.failing.lock().unwrap() = failing;
    }
}

impl LightingOutputs for RecordingOutputs {
    fn set_levels(&mut self, levels: ChannelLevels) -> Result<(), &'static str> {
        if *self.failing.lock().unwrap() {
            return Err("refused");
        }
        self.written.lock().unwrap().push(levels);
        Ok(())
    }
}

struct RefusingStore;

impl LightingPersistence for RefusingStore {
    fn load(&self) -> Option<LightingPalette> {
        None
    }

    fn store(&self, _value: &LightingPalette) -> Result<(), &'static str> {
        Err("flash is full")
    }
}

fn service() -> Arc<LightingService<VolatileLightingStore>> {
    Arc::new(LightingService::new(
        LightingPalette::DEFAULT,
        VolatileLightingStore::default(),
    ))
}

const MOON: ChannelLevels = ChannelLevels::new(1, 2, 3, 4);

#[test]
fn a_stored_palette_wins_over_the_defaults() {
    let store = VolatileLightingStore::default();
    let stored = LightingPalette {
        moonlight: MOON,
        ..LightingPalette::DEFAULT
    };
    store.store(&stored).unwrap();

    let service = LightingService::new(LightingPalette::DEFAULT, store);
    assert_eq!(service.palette(), stored);
}

#[test]
fn an_invalid_stored_palette_falls_back_to_the_defaults() {
    let store = VolatileLightingStore::default();
    let mut stored = LightingPalette::DEFAULT;
    stored.sunset.red_pct = 200;
    store.store(&stored).unwrap();

    let service = LightingService::new(LightingPalette::DEFAULT, store);
    assert_eq!(service.palette(), LightingPalette::DEFAULT);
}

#[test]
fn a_valid_patch_is_stored_and_applied() {
    let service = service();
    let patch = LightingPalettePatch {
        moonlight: Some(MOON),
        ..LightingPalettePatch::default()
    };

    let applied = service.apply_palette(patch).expect("accepted");
    assert_eq!(applied.moonlight, MOON);
    assert_eq!(service.palette().moonlight, MOON);
}

#[test]
fn an_out_of_range_patch_is_rejected_and_changes_nothing() {
    let service = service();
    let patch = LightingPalettePatch {
        daylight: Some(ChannelLevels::new(0, 0, 0, 101)),
        ..LightingPalettePatch::default()
    };

    assert!(matches!(
        service.apply_palette(patch),
        Err(LightingPaletteError::Invalid(_))
    ));
    assert_eq!(service.palette(), LightingPalette::DEFAULT);
}

#[test]
fn a_patch_that_cannot_be_stored_changes_nothing() {
    let service = LightingService::new(LightingPalette::DEFAULT, RefusingStore);
    let patch = LightingPalettePatch {
        moonlight: Some(MOON),
        ..LightingPalettePatch::default()
    };

    assert!(matches!(
        service.apply_palette(patch),
        Err(LightingPaletteError::NotPersisted(_))
    ));
    assert_eq!(service.palette(), LightingPalette::DEFAULT);
}

#[test]
fn status_is_unavailable_until_something_is_published() {
    assert!(!service().lighting_status().available);
}

#[tokio::test(start_paused = true)]
async fn a_timed_override_expires() {
    let service = service();
    service.set_override(LightingTarget::Profile(LightingProfile::Off), Some(10));
    assert!(service.current_override().is_some());

    tokio::time::sleep(Duration::from_secs(11)).await;
    assert!(service.current_override().is_none());
}

async fn settle() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

fn spawn(
    service: &Arc<LightingService<VolatileLightingStore>>,
    outputs: &RecordingOutputs,
    republish: &Arc<Notify>,
) {
    tokio::task::spawn_local(lighting_loop(
        NoonInJune,
        Arc::clone(service),
        outputs.clone(),
        Arc::clone(republish),
        Duration::from_secs(30),
    ));
}

#[tokio::test(start_paused = true)]
async fn the_loop_applies_the_schedule_and_publishes_it() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            let republish = Arc::new(Notify::new());
            spawn(&service, &outputs, &republish);
            settle().await;

            assert_eq!(outputs.written(), vec![LightingPalette::DEFAULT.daylight]);
            let status = service.lighting_status();
            assert!(status.available);
            assert_eq!(status.profile, LightingProfile::Daylight);
            assert_eq!(status.channels, LightingPalette::DEFAULT.daylight);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn unchanged_levels_are_not_rewritten_on_the_next_tick() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;

            tokio::time::sleep(Duration::from_secs(31)).await;
            settle().await;

            assert_eq!(outputs.written().len(), 1);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_override_is_applied_without_waiting_for_the_tick() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;

            service.set_override(LightingTarget::Channels(MOON), None);
            settle().await;

            assert_eq!(outputs.written().last(), Some(&MOON));
            assert!(service.lighting_status().override_active);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_palette_change_is_applied_without_waiting_for_the_tick() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;

            service
                .apply_palette(LightingPalettePatch {
                    daylight: Some(MOON),
                    ..LightingPalettePatch::default()
                })
                .unwrap();
            settle().await;

            assert_eq!(outputs.written().last(), Some(&MOON));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_timed_override_ends_on_time() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;

            service.set_override(LightingTarget::Channels(MOON), Some(5));
            settle().await;
            tokio::time::sleep(Duration::from_secs(7)).await;
            settle().await;

            assert_eq!(
                outputs.written().last(),
                Some(&LightingPalette::DEFAULT.daylight)
            );
            assert!(!service.lighting_status().override_active);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_write_is_retried_and_not_published() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            outputs.fail(true);
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;

            assert!(!service.lighting_status().available);

            outputs.fail(false);
            tokio::time::sleep(Duration::from_secs(31)).await;
            settle().await;

            assert_eq!(outputs.written(), vec![LightingPalette::DEFAULT.daylight]);
            assert!(service.lighting_status().available);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn the_first_apply_asks_for_a_republish() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            let republish = Arc::new(Notify::new());
            spawn(&service, &outputs, &republish);
            settle().await;

            assert!(timeout(Duration::from_millis(1), republish.notified())
                .await
                .is_ok());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unchanged_tick_does_not_ask_for_a_republish() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            let republish = Arc::new(Notify::new());
            spawn(&service, &outputs, &republish);
            settle().await;
            timeout(Duration::from_millis(1), republish.notified())
                .await
                .expect("first apply notifies");

            tokio::time::sleep(Duration::from_secs(31)).await;
            settle().await;

            assert!(timeout(Duration::from_millis(1), republish.notified())
                .await
                .is_err());
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn clearing_an_override_restores_the_schedule_without_waiting_for_the_tick() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;
            service.set_override(LightingTarget::Channels(MOON), None);
            settle().await;
            assert_eq!(outputs.written().last(), Some(&MOON));

            service.clear_override();
            settle().await;

            assert_eq!(
                outputs.written().last(),
                Some(&LightingPalette::DEFAULT.daylight)
            );
            let status = service.lighting_status();
            assert!(!status.override_active);
            assert_eq!(status.profile, LightingProfile::Daylight);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_refused_write_keeps_the_previously_published_status() {
    LocalSet::new()
        .run_until(async {
            let service = service();
            let outputs = RecordingOutputs::default();
            spawn(&service, &outputs, &Arc::new(Notify::new()));
            settle().await;
            let published = service.lighting_status();

            outputs.fail(true);
            service.set_override(LightingTarget::Channels(MOON), None);
            settle().await;

            assert_eq!(service.lighting_status(), published);
        })
        .await;
}
