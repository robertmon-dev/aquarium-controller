use std::sync::Arc;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::application::pump::PumpService;
use crate::domain::pump::{
    ChannelMode, OperationKind, OperationState, OverrideError, PumpId, PumpSettings,
    PumpSettingsPatch, RuntimeCounters, StoredOverride, Supervision, TopOffWindow,
};
use crate::ports::inbound::{PumpControl, PumpSettingsError};
use crate::ports::outbound::{PumpRecord, PumpStatusSource, PumpStore};

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
fn resetting_a_counter_stamps_the_time() {
    let (service, store) = service_and_store();

    service.reset_runtime(PumpId::Uv);

    let uv = service.status().channels.uv;
    assert_eq!(uv.runtime_s, 0);
    assert_eq!(uv.runtime_reset_at, Some(EPOCH));

    let counters: RuntimeCounters = decode(&store, PumpRecord::Runtime).expect("stored");
    assert_eq!(counters.reset_at[PumpId::Uv.index()], Some(EPOCH));
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

#[test]
fn water_change_activity_follows_only_a_water_change() {
    use crate::ports::outbound::WaterChangeActivity;

    let mut rig = Rig::new();
    rig.run(5, Some(&healthy()));
    assert!(!rig.service.water_change_active());

    rig.service.start_top_off(60).expect("top-off");
    assert!(!rig.service.water_change_active());
    rig.service
        .cancel(OperationKind::TopOff)
        .expect("cancelled");

    rig.service.start_water_change(PLAN).expect("water change");
    assert!(rig.service.water_change_active());
    rig.service
        .cancel(OperationKind::WaterChange)
        .expect("cancelled");
    assert!(!rig.service.water_change_active());
}

#[test]
fn a_resuming_water_change_is_active() {
    use crate::ports::outbound::WaterChangeActivity;

    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 20), Some(EPOCH - 10));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);
    rig.service.set_supervision(Supervision::Unavailable);

    rig.run(3, Some(&healthy()));

    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Resuming
    );
    assert!(rig.service.water_change_active());
}

#[test]
fn a_suspended_water_change_is_not_active() {
    use crate::ports::outbound::WaterChangeActivity;

    let store = Arc::new(VolatilePumpStore::default());
    seed_checkpoint(&store, interrupted_drain(EPOCH - 9000), Some(EPOCH - 8000));
    let mut rig = Rig::build(store, TestClock::synced(EPOCH), None);

    rig.run(5, Some(&healthy()));

    assert_eq!(
        rig.service.status().operation.state,
        OperationState::Suspended
    );
    assert!(!rig.service.water_change_active());
}
