use std::sync::Arc;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{ChannelMode, PumpId, StoredOverride};
use crate::ports::inbound::PumpControl;
use crate::ports::outbound::{Clock, PumpRecord};

#[test]
fn a_timed_uv_override_expires_with_the_clock() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    rig.set_override(PumpId::Uv, Some(60));
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

fn step_clock(rig: &Rig, seconds: i64) {
    let now = rig.clock.epoch_seconds().expect("synced");
    rig.clock
        .set_epoch(Some(now.saturating_add_signed(seconds)));
}

#[test]
fn a_timed_override_lasts_its_duration_when_the_clock_steps() {
    for jump in [-7200, -300, 300, 7200] {
        let mut rig = Rig::new();
        let healthy = healthy();
        rig.tick(Some(&healthy));
        rig.set_override(PumpId::Uv, Some(600));
        rig.run(100, Some(&healthy));

        step_clock(&rig, jump);
        rig.run(499, Some(&healthy));
        assert!(!rig.pins()[1], "{jump:+}");

        rig.run(1, Some(&healthy));
        assert!(rig.pins()[1], "{jump:+}");
        assert_eq!(
            decode::<[Option<StoredOverride>; 2]>(&rig.store, PumpRecord::Overrides),
            Some([None, None]),
            "{jump:+}"
        );
    }
}

#[test]
fn a_return_override_set_this_boot_survives_a_lost_time_sync() {
    let mut rig = Rig::new();
    let healthy = healthy();
    rig.tick(Some(&healthy));
    rig.set_override(PumpId::Return, Some(120));
    rig.run(10, Some(&healthy));

    rig.clock.set_epoch(None);
    rig.run(10, Some(&healthy));
    assert_eq!(
        rig.service.status().channels.return_pump.mode,
        ChannelMode::ManualOff
    );
    assert!(!rig.pins()[0]);

    rig.run(100, Some(&healthy));
    assert!(rig.pins()[0]);
}
