use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::*;
use crate::adapters::outbound::memory::VolatileHeaterStore;
use crate::domain::heater::{HeaterFault, HeaterMode, HeaterSettingsPatch, ZoneSettings};
use crate::ports::inbound::{HeaterControl, HeaterSettingsError};
use crate::ports::outbound::HeaterOutputs;
use crate::ports::outbound::HeaterStatusSource;
use crate::ports::outbound::TemperatureSample;
use tokio::sync::{watch, Notify};

fn defaults() -> HeaterSettings {
    HeaterSettings {
        tank: ZoneSettings {
            target_celsius: 27.0,
            hysteresis_celsius: 0.5,
        },
        sump: ZoneSettings {
            target_celsius: 26.0,
            hysteresis_celsius: 0.5,
        },
        cutoff_celsius: 30.0,
    }
}

#[derive(Default)]
struct FakePersistence {
    stored: Mutex<Option<HeaterSettings>>,
    fail: bool,
}

impl FakePersistence {
    fn holding(settings: HeaterSettings) -> Arc<Self> {
        Arc::new(Self {
            stored: Mutex::new(Some(settings)),
            fail: false,
        })
    }

    fn broken() -> Arc<Self> {
        Arc::new(Self {
            stored: Mutex::new(None),
            fail: true,
        })
    }

    fn stored(&self) -> Option<HeaterSettings> {
        *self.stored.lock().unwrap()
    }
}

impl HeaterPersistence for Arc<FakePersistence> {
    fn load(&self) -> Option<HeaterSettings> {
        self.stored()
    }

    fn store(&self, value: &HeaterSettings) -> Result<(), &'static str> {
        if self.fail {
            return Err("fake: NVS is full");
        }

        *self.stored.lock().unwrap() = Some(*value);
        Ok(())
    }
}

fn service() -> HeaterService<Arc<FakePersistence>> {
    HeaterService::new(defaults(), Arc::new(FakePersistence::default()))
}

#[tokio::test(start_paused = true)]
async fn nothing_stored_means_the_defaults() {
    assert_eq!(service().settings(), defaults());
}

#[tokio::test(start_paused = true)]
async fn stored_settings_win_over_the_defaults() {
    let mut stored = defaults();
    stored.cutoff_celsius = 29.0;

    let service = HeaterService::new(defaults(), FakePersistence::holding(stored));

    assert_eq!(service.settings(), stored);
}

#[tokio::test(start_paused = true)]
async fn stored_settings_that_fail_validation_are_ignored() {
    let mut stored = defaults();
    stored.cutoff_celsius = 20.0;

    let service = HeaterService::new(defaults(), FakePersistence::holding(stored));

    assert_eq!(service.settings(), defaults());
}

#[tokio::test(start_paused = true)]
async fn a_patch_is_merged_validated_and_persisted() {
    let persistence = Arc::new(FakePersistence::default());
    let service = HeaterService::new(defaults(), Arc::clone(&persistence));

    let applied = service
        .apply_settings(HeaterSettingsPatch {
            cutoff_celsius: Some(29.5),
            ..HeaterSettingsPatch::default()
        })
        .expect("a valid patch");

    assert_eq!(applied.cutoff_celsius, 29.5);
    assert_eq!(applied.tank, defaults().tank);
    assert_eq!(service.settings(), applied);
    assert_eq!(persistence.stored(), Some(applied));
}

#[tokio::test(start_paused = true)]
async fn an_invalid_patch_changes_nothing() {
    let service = service();

    let result = service.apply_settings(HeaterSettingsPatch {
        cutoff_celsius: Some(26.5),
        ..HeaterSettingsPatch::default()
    });

    assert!(matches!(result, Err(HeaterSettingsError::Invalid(_))));
    assert_eq!(service.settings(), defaults());
}

#[tokio::test(start_paused = true)]
async fn a_failed_write_keeps_the_previous_settings() {
    let service = HeaterService::new(defaults(), FakePersistence::broken());

    let result = service.apply_settings(HeaterSettingsPatch {
        cutoff_celsius: Some(29.5),
        ..HeaterSettingsPatch::default()
    });

    assert!(matches!(result, Err(HeaterSettingsError::NotPersisted(_))));
    assert_eq!(service.settings(), defaults());
}

#[tokio::test(start_paused = true)]
async fn status_is_unavailable_until_the_controller_records_a_tick() {
    let service = service();

    let status = service.heater_status();

    assert_eq!(status.tank.fault, Some(HeaterFault::Unavailable));
    assert!(!status.tank.active);
    assert_eq!(status.tank.target_celsius, 27.0);
    assert_eq!(status.sump.target_celsius, 26.0);
}

#[tokio::test(start_paused = true)]
async fn recorded_outputs_show_up_in_the_status() {
    let service = service();

    assert!(service.record(HeaterId::Tank, true, None));
    assert!(service.record(HeaterId::Sump, false, Some(HeaterFault::SensorFault)));

    let status = service.status();
    assert!(status.tank.active);
    assert_eq!(status.tank.fault, None);
    assert_eq!(status.sump.fault, Some(HeaterFault::SensorFault));
    assert!(service.is_on(HeaterId::Tank));
}

#[tokio::test(start_paused = true)]
async fn recording_the_same_output_twice_is_not_a_change() {
    let service = service();
    service.record(HeaterId::Tank, true, None);

    assert!(!service.record(HeaterId::Tank, true, None));
    assert!(service.record(HeaterId::Tank, false, None));
}

#[tokio::test(start_paused = true)]
async fn an_on_override_expires_after_its_duration() {
    let service = service();

    let status = service
        .set_override(HeaterId::Tank, OverrideState::On, Some(600))
        .expect("valid override");

    assert_eq!(status.mode, HeaterMode::ManualOn);
    assert_eq!(status.override_expires_in_s, Some(600));
    assert_eq!(
        service.active_override(HeaterId::Tank),
        Some(OverrideState::On)
    );

    tokio::time::advance(Duration::from_secs(599)).await;
    assert_eq!(service.status().tank.override_expires_in_s, Some(1));

    tokio::time::advance(Duration::from_secs(1)).await;
    assert_eq!(service.active_override(HeaterId::Tank), None);
    assert_eq!(service.status().tank.mode, HeaterMode::Auto);
}

#[tokio::test(start_paused = true)]
async fn an_off_override_without_duration_lasts_until_cleared() {
    let service = service();
    service
        .set_override(HeaterId::Sump, OverrideState::Off, None)
        .expect("valid override");

    tokio::time::advance(Duration::from_secs(86_400 * 30)).await;

    let status = service.status().sump;
    assert_eq!(status.mode, HeaterMode::ManualOff);
    assert_eq!(status.override_expires_in_s, None);

    service.clear_override(HeaterId::Sump);
    assert_eq!(service.status().sump.mode, HeaterMode::Auto);
}

#[tokio::test(start_paused = true)]
async fn an_invalid_override_is_refused_and_not_stored() {
    let service = service();

    assert!(service
        .set_override(HeaterId::Tank, OverrideState::On, None)
        .is_err());
    assert_eq!(service.active_override(HeaterId::Tank), None);
}

#[tokio::test(start_paused = true)]
async fn overrides_and_settings_wake_the_change_signal() {
    let service = service();
    let signal = service.change_signal();

    service
        .set_override(HeaterId::Tank, OverrideState::Off, None)
        .unwrap();
    tokio::time::timeout(Duration::from_millis(10), signal.notified())
        .await
        .expect("set_override must notify");

    service.clear_override(HeaterId::Tank);
    tokio::time::timeout(Duration::from_millis(10), signal.notified())
        .await
        .expect("clear_override must notify");

    service
        .apply_settings(HeaterSettingsPatch::default())
        .unwrap();
    tokio::time::timeout(Duration::from_millis(10), signal.notified())
        .await
        .expect("apply_settings must notify");
}

#[tokio::test(start_paused = true)]
async fn recording_one_heater_leaves_the_other_unavailable() {
    let service = service();
    service.record(HeaterId::Tank, true, None);

    assert_eq!(service.status().sump.fault, Some(HeaterFault::Unavailable));
    assert!(service.record(HeaterId::Sump, false, None));
}

#[tokio::test(start_paused = true)]
async fn an_off_override_with_a_duration_expires_back_to_auto() {
    let service = service();
    service
        .set_override(HeaterId::Tank, OverrideState::Off, Some(60))
        .unwrap();

    assert_eq!(service.status().tank.mode, HeaterMode::ManualOff);

    tokio::time::advance(Duration::from_secs(60)).await;
    assert_eq!(service.status().tank.mode, HeaterMode::Auto);
}

#[tokio::test(start_paused = true)]
async fn an_override_on_one_heater_leaves_the_other_in_auto() {
    let service = service();
    service
        .set_override(HeaterId::Tank, OverrideState::Off, None)
        .unwrap();

    assert_eq!(service.status().sump.mode, HeaterMode::Auto);
    assert_eq!(service.active_override(HeaterId::Sump), None);
}

#[tokio::test(start_paused = true)]
async fn an_override_before_any_record_is_manual_and_unavailable() {
    let service = service();

    let status = service
        .set_override(HeaterId::Tank, OverrideState::Off, None)
        .unwrap();

    assert_eq!(status.mode, HeaterMode::ManualOff);
    assert_eq!(status.fault, Some(HeaterFault::Unavailable));
}

#[tokio::test(start_paused = true)]
async fn remaining_seconds_round_up_so_an_active_override_never_reads_zero() {
    let service = service();
    service
        .set_override(HeaterId::Tank, OverrideState::On, Some(600))
        .unwrap();

    tokio::time::advance(Duration::from_millis(599_500)).await;

    assert_eq!(service.status().tank.override_expires_in_s, Some(1));
}

#[derive(Clone, Default)]
struct FakeOutputs {
    log: Arc<Mutex<Vec<(HeaterId, bool)>>>,
    pins: Arc<Mutex<[bool; 2]>>,
    refusing_off: Arc<Mutex<Option<HeaterId>>>,
    stuck: Option<HeaterId>,
    broken: bool,
}

impl FakeOutputs {
    fn stuck(id: HeaterId) -> Self {
        Self {
            stuck: Some(id),
            ..Self::default()
        }
    }

    fn broken() -> Self {
        Self {
            broken: true,
            ..Self::default()
        }
    }

    fn pin(&self, id: HeaterId) -> bool {
        self.pins.lock().unwrap()[id.index()]
    }

    fn refuse_off(&self, id: Option<HeaterId>) {
        *self.refusing_off.lock().unwrap() = id;
    }

    fn writes(&self, id: HeaterId) -> Vec<bool> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|(heater, _)| *heater == id)
            .map(|(_, on)| *on)
            .collect()
    }

    fn last(&self, id: HeaterId) -> Option<bool> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .rev()
            .find(|(heater, _)| *heater == id)
            .map(|(_, on)| *on)
    }
}

impl HeaterOutputs for FakeOutputs {
    fn set(&mut self, id: HeaterId, on: bool) -> Result<(), &'static str> {
        self.log.lock().unwrap().push((id, on));

        let refused_off = !on && *self.refusing_off.lock().unwrap() == Some(id);

        if self.broken || (on && self.stuck == Some(id)) || refused_off {
            Err("fake: the SSR pin is stuck")
        } else {
            self.pins.lock().unwrap()[id.index()] = on;
            Ok(())
        }
    }
}

type TestService = HeaterService<VolatileHeaterStore>;

fn shared_service() -> Arc<TestService> {
    Arc::new(HeaterService::new(defaults(), VolatileHeaterStore))
}

fn snapshot(tank: Option<f32>, sump: Option<f32>) -> TemperatureSample {
    TemperatureSample {
        tank_celsius: tank,
        sump_celsius: sump,
        measured_at: Instant::now(),
    }
}

#[tokio::test(start_paused = true)]
async fn a_cold_tank_and_a_warm_sump_drive_their_own_heaters() {
    let outputs = FakeOutputs::default();
    let service = shared_service();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));

    controller.tick(Some(&snapshot(Some(26.0), Some(26.5))));

    assert_eq!(outputs.last(HeaterId::Tank), Some(true));
    assert_eq!(outputs.last(HeaterId::Sump), Some(false));

    let status = service.status();
    assert!(status.tank.active);
    assert!(!status.sump.active);
    assert_eq!(status.tank.fault, None);
}

#[tokio::test(start_paused = true)]
async fn the_hysteresis_band_holds_across_ticks() {
    let outputs = FakeOutputs::default();
    let mut controller = HeaterController::new(outputs.clone(), shared_service());

    controller.tick(Some(&snapshot(Some(26.4), Some(26.0))));
    assert_eq!(outputs.last(HeaterId::Tank), Some(true));

    controller.tick(Some(&snapshot(Some(26.8), Some(26.0))));
    assert_eq!(outputs.last(HeaterId::Tank), Some(true));

    controller.tick(Some(&snapshot(Some(27.0), Some(26.0))));
    assert_eq!(outputs.last(HeaterId::Tank), Some(false));

    controller.tick(Some(&snapshot(Some(26.8), Some(26.0))));
    assert_eq!(outputs.last(HeaterId::Tank), Some(false));
}

#[tokio::test(start_paused = true)]
async fn the_cutoff_beats_a_manual_on_override() {
    let outputs = FakeOutputs::default();
    let service = shared_service();
    service
        .set_override(HeaterId::Tank, OverrideState::On, Some(600))
        .unwrap();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));

    controller.tick(Some(&snapshot(Some(30.5), Some(26.0))));

    assert_eq!(outputs.last(HeaterId::Tank), Some(false));
    let status = service.status().tank;
    assert_eq!(status.mode, HeaterMode::ManualOn);
    assert_eq!(status.fault, Some(HeaterFault::OverTemperature));
    assert!(!status.active);
}

#[tokio::test(start_paused = true)]
async fn no_snapshot_turns_both_heaters_off_with_a_sensor_fault() {
    let outputs = FakeOutputs::default();
    let service = shared_service();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));
    controller.tick(Some(&snapshot(Some(20.0), Some(20.0))));

    controller.tick(None);

    for id in HeaterId::ALL {
        assert_eq!(outputs.last(id), Some(false));
        assert_eq!(
            service.status().get(id).fault,
            Some(HeaterFault::SensorFault)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_stuck_output_is_driven_off_and_reported_inactive() {
    let outputs = FakeOutputs::stuck(HeaterId::Tank);
    let service = shared_service();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));

    controller.tick(Some(&snapshot(Some(20.0), Some(20.0))));

    assert_eq!(outputs.last(HeaterId::Tank), Some(false));
    assert!(!service.status().tank.active);
    assert!(service.status().sump.active);
    assert_eq!(service.status().tank.fault, Some(HeaterFault::OutputError));
    assert_eq!(service.status().sump.fault, None);
    assert!(!controller.tick(Some(&snapshot(Some(20.0), Some(20.0)))));
}

#[tokio::test(start_paused = true)]
async fn an_output_that_fails_even_to_turn_off_is_reported_inactive() {
    let outputs = FakeOutputs::broken();
    let service = shared_service();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));

    controller.tick(Some(&snapshot(Some(20.0), Some(20.0))));

    let status = service.status();
    assert!(!status.tank.active);
    assert!(!status.sump.active);
    assert_eq!(status.tank.fault, Some(HeaterFault::OutputError));
    assert_eq!(status.sump.fault, Some(HeaterFault::OutputError));
}

#[tokio::test(start_paused = true)]
async fn a_refused_off_write_reports_the_heater_still_on_and_retries_off_every_tick() {
    let outputs = FakeOutputs::default();
    let service = shared_service();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));
    controller.tick(Some(&snapshot(Some(20.0), Some(27.0))));
    assert!(outputs.pin(HeaterId::Tank));

    outputs.refuse_off(Some(HeaterId::Tank));
    assert!(controller.tick(Some(&snapshot(Some(27.5), Some(27.0)))));

    assert!(outputs.pin(HeaterId::Tank));
    let status = service.status().tank;
    assert!(status.active);
    assert_eq!(status.fault, Some(HeaterFault::OutputError));

    let before = outputs.writes(HeaterId::Tank).len();
    controller.tick(Some(&snapshot(Some(20.0), Some(27.0))));
    assert_eq!(&outputs.writes(HeaterId::Tank)[before..], &[false]);
    assert_eq!(service.status().tank.fault, Some(HeaterFault::OutputError));
    assert!(service.status().tank.active);

    outputs.refuse_off(None);
    controller.tick(Some(&snapshot(Some(27.5), Some(27.0))));

    assert!(!outputs.pin(HeaterId::Tank));
    let status = service.status().tank;
    assert!(!status.active);
    assert_eq!(status.fault, None);
}

#[tokio::test(start_paused = true)]
async fn a_refused_off_on_a_safety_trip_is_reported_as_an_output_error() {
    let outputs = FakeOutputs::default();
    let service = shared_service();
    let mut controller = HeaterController::new(outputs.clone(), Arc::clone(&service));
    controller.tick(Some(&snapshot(Some(20.0), Some(27.0))));

    outputs.refuse_off(Some(HeaterId::Tank));
    controller.tick(None);

    let status = service.status().tank;
    assert!(status.active);
    assert_eq!(status.fault, Some(HeaterFault::OutputError));
}

#[tokio::test(start_paused = true)]
async fn tick_reports_a_change_only_when_an_output_or_fault_moved() {
    let mut controller = HeaterController::new(FakeOutputs::default(), shared_service());

    assert!(controller.tick(Some(&snapshot(Some(20.0), Some(20.0)))));
    assert!(!controller.tick(Some(&snapshot(Some(20.5), Some(20.5)))));
    assert!(controller.tick(Some(&snapshot(Some(28.0), Some(20.5)))));
}

const WATCHDOG: Duration = Duration::from_secs(6);

struct Loop {
    tx: watch::Sender<Option<TemperatureSample>>,
    outputs: FakeOutputs,
    service: Arc<TestService>,
    republish: Arc<Notify>,
    handle: tokio::task::JoinHandle<()>,
}

fn spawn_loop() -> Loop {
    let (tx, rx) = watch::channel(None);
    let outputs = FakeOutputs::default();
    let service = shared_service();
    let republish = Arc::new(Notify::new());

    let handle = tokio::spawn(heater_loop(
        HeaterController::new(outputs.clone(), Arc::clone(&service)),
        rx,
        service.change_signal(),
        Arc::clone(&republish),
        WATCHDOG,
    ));

    Loop {
        tx,
        outputs,
        service,
        republish,
        handle,
    }
}

async fn expect_republish(republish: &Notify) {
    tokio::time::timeout(Duration::from_secs(1), republish.notified())
        .await
        .expect("the loop must request a republish");
}

#[tokio::test(start_paused = true)]
async fn a_fresh_snapshot_drives_the_outputs_and_requests_a_republish() {
    let subject = spawn_loop();

    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(27.0))));
    expect_republish(&subject.republish).await;

    assert_eq!(subject.outputs.last(HeaterId::Tank), Some(true));
    assert_eq!(subject.outputs.last(HeaterId::Sump), Some(false));
}

#[tokio::test(start_paused = true)]
async fn silence_past_the_watchdog_turns_both_heaters_off() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(20.0))));
    expect_republish(&subject.republish).await;

    let early = tokio::time::timeout(
        WATCHDOG - Duration::from_millis(1),
        subject.republish.notified(),
    )
    .await;
    assert!(early.is_err(), "the watchdog must not fire early");

    tokio::time::timeout(Duration::from_millis(2), subject.republish.notified())
        .await
        .expect("the watchdog tick must request a republish");

    for id in HeaterId::ALL {
        assert_eq!(subject.outputs.last(id), Some(false));
        assert_eq!(
            subject.service.status().get(id).fault,
            Some(HeaterFault::SensorFault)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn an_api_change_applies_at_once_to_the_latest_snapshot() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(20.0))));
    expect_republish(&subject.republish).await;

    subject
        .service
        .set_override(HeaterId::Tank, OverrideState::Off, None)
        .unwrap();
    expect_republish(&subject.republish).await;

    assert_eq!(subject.outputs.last(HeaterId::Tank), Some(false));
    assert_eq!(subject.outputs.last(HeaterId::Sump), Some(true));
}

#[tokio::test(start_paused = true)]
async fn an_unchanged_snapshot_does_not_request_a_republish() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(20.0))));
    expect_republish(&subject.republish).await;

    subject
        .tx
        .send_replace(Some(snapshot(Some(20.1), Some(20.1))));

    let quiet = tokio::time::timeout(Duration::from_secs(1), subject.republish.notified()).await;
    assert!(
        quiet.is_err(),
        "a republish here would feed back into the loop"
    );
}

#[tokio::test(start_paused = true)]
async fn a_closed_telemetry_channel_leaves_both_heaters_off() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(20.0))));
    expect_republish(&subject.republish).await;

    drop(subject.tx);
    subject.handle.await.expect("the loop returns cleanly");

    for id in HeaterId::ALL {
        assert_eq!(subject.outputs.last(id), Some(false));
    }
}

#[tokio::test(start_paused = true)]
async fn an_api_wake_requests_a_republish_even_when_no_output_changes() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(20.0))));
    expect_republish(&subject.republish).await;

    subject
        .service
        .apply_settings(HeaterSettingsPatch::default())
        .unwrap();

    expect_republish(&subject.republish).await;
}

#[tokio::test(start_paused = true)]
async fn api_wakes_cannot_postpone_the_watchdog() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(20.0), Some(20.0))));
    expect_republish(&subject.republish).await;

    for round in 0..3 {
        tokio::time::advance(Duration::from_secs(2)).await;

        if round % 2 == 0 {
            subject
                .service
                .set_override(HeaterId::Tank, OverrideState::Off, None)
                .unwrap();
        } else {
            subject.service.clear_override(HeaterId::Tank);
        }

        expect_republish(&subject.republish).await;
    }

    tokio::time::advance(Duration::from_millis(1)).await;
    tokio::task::yield_now().await;

    for id in HeaterId::ALL {
        assert_eq!(subject.outputs.last(id), Some(false));
        assert_eq!(
            subject.service.status().get(id).fault,
            Some(HeaterFault::SensorFault)
        );
    }
}

#[tokio::test(start_paused = true)]
async fn a_timed_on_override_ends_on_time_without_a_new_sample() {
    let subject = spawn_loop();
    subject
        .tx
        .send_replace(Some(snapshot(Some(28.0), Some(28.0))));
    expect_republish(&subject.republish).await;

    subject
        .service
        .set_override(HeaterId::Tank, OverrideState::On, Some(1))
        .unwrap();
    expect_republish(&subject.republish).await;
    assert!(subject.service.status().tank.active);

    tokio::time::advance(Duration::from_millis(1_001)).await;
    tokio::task::yield_now().await;

    assert!(!subject.service.status().tank.active);
    assert_eq!(subject.outputs.last(HeaterId::Tank), Some(false));
    assert_eq!(subject.service.status().tank.mode, HeaterMode::Auto);
}

#[tokio::test(start_paused = true)]
async fn a_republished_sample_keeps_its_original_age() {
    let subject = spawn_loop();
    let cold = snapshot(Some(20.0), Some(20.0));
    subject.tx.send_replace(Some(cold));
    expect_republish(&subject.republish).await;
    assert!(subject.service.status().tank.active);

    tokio::time::advance(Duration::from_millis(1_900)).await;
    subject.tx.send_replace(Some(cold));
    tokio::task::yield_now().await;

    tokio::time::advance(Duration::from_millis(4_200)).await;
    tokio::task::yield_now().await;

    for id in HeaterId::ALL {
        assert_eq!(
            subject.service.status().get(id).fault,
            Some(HeaterFault::SensorFault)
        );
        assert!(!subject.service.status().get(id).active);
        assert_eq!(subject.outputs.last(id), Some(false));
    }
}

#[tokio::test(start_paused = true)]
async fn a_sample_that_arrives_already_stale_is_not_trusted() {
    let subject = spawn_loop();
    let measured = snapshot(Some(20.0), Some(20.0));

    tokio::time::advance(WATCHDOG).await;
    subject.tx.send_replace(Some(measured));
    tokio::task::yield_now().await;

    assert_eq!(
        subject.service.status().tank.fault,
        Some(HeaterFault::SensorFault)
    );
    assert_eq!(subject.outputs.last(HeaterId::Tank), Some(false));
}
