use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::adapters::outbound::memory::VolatilePumpStore;
use crate::application::pump::fixtures::*;
use crate::domain::pump::{
    Blocker, ChannelFault, ChannelMode, OperationResult, OperationStatus, PumpId, PumpSettings,
    Supervision,
};
use crate::ports::inbound::{PumpControl, StartError};
use crate::ports::outbound::{PumpRecord, PumpStatusSource};

#[test]
fn a_fresh_service_reports_unavailable_channels_and_no_operation() {
    let (service, _store) = service_and_store();
    let status = service.status();

    for id in PumpId::ALL {
        assert_eq!(
            status.channels.get(id).fault,
            Some(ChannelFault::Unavailable)
        );
    }

    assert_eq!(status.operation, OperationStatus::IDLE);
    assert_eq!(service.settings(), PumpSettings::DEFAULT);
}

#[test]
fn unreadable_records_fall_back_to_defaults() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(PumpRecord::Settings, b"not json".to_vec());
    store.put(PumpRecord::Checkpoint, b"{".to_vec());

    let service = service_on(&store, TestClock::synced(EPOCH));

    assert_eq!(service.settings(), PumpSettings::DEFAULT);
    assert_eq!(service.status().operation, OperationStatus::IDLE);
}

#[test]
fn a_runtime_reset_credits_only_the_time_after_it() {
    let mut rig = Rig::new();
    let healthy = healthy();

    rig.tick(Some(&healthy));
    let start = rig.now;
    rig.now += Duration::from_secs(10);
    rig.service
        .reset_runtime_at(PumpId::Uv, start + Duration::from_secs(8));
    rig.tick(Some(&healthy));

    let status = rig.service.status().channels;
    assert_eq!(status.uv.runtime_s, 2);
    assert_eq!(status.return_pump.runtime_s, 10);
}

const DRAINED_HISTORY_WITH_AN_UNKNOWN_RESULT: &[u8] = br#"{"top_off":null,
    "water_change":{"kind":"water_change","started_at":1700000000,"drain_ran_s":3,
    "settle_ran_s":2,"refill_ran_s":4,"result":"exploded","resumed":false},
    "drained_at":1700000000}"#;

#[test]
fn the_gap_survives_an_unreadable_water_change_record() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::LastOperations,
        DRAINED_HISTORY_WITH_AN_UNKNOWN_RESULT.to_vec(),
    );
    let service = service_on(&store, TestClock::synced(EPOCH + 3600));
    observe(&service, Some(HEALTHY));

    assert_eq!(service.status().last_water_change, None);
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn one_unreadable_history_entry_keeps_the_others() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::LastOperations,
        br#"{"top_off":{"kind":"top_off","started_at":1,"drain_ran_s":0,"settle_ran_s":0,
            "refill_ran_s":60,"result":"exploded","resumed":false},
            "water_change":{"kind":"water_change","started_at":1700000000,"drain_ran_s":3,
            "settle_ran_s":2,"refill_ran_s":4,"result":"sump_full","resumed":false}}"#
            .to_vec(),
    );
    let service = service_on(&store, TestClock::synced(EPOCH + 3600));
    observe(&service, Some(HEALTHY));

    let status = service.status();
    assert_eq!(status.last_top_off, None);
    assert_eq!(
        status.last_water_change.map(|record| record.result),
        Some(OperationResult::SumpFull)
    );
    assert_eq!(
        service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::RecentWaterChange))
    );
}

#[test]
fn a_stored_override_without_an_expiry_is_dropped() {
    for (stored, uv_off) in [
        (&br#"[null,{}]"#[..], false),
        (br#"[null,{"expires_at":null}]"#, true),
        (br#"[null,{"expires_at":"soon"}]"#, false),
        (br#"[{},{"expires_at":null}]"#, true),
        (br#"[null,{"expires_at":null},null]"#, true),
        (br#"[{"expires_at":null}]"#, false),
    ] {
        let store = Arc::new(VolatilePumpStore::default());
        store.put(PumpRecord::Overrides, stored.to_vec());
        let service = service_on(&store, TestClock::synced(EPOCH));
        let status = service.status().channels;

        let label = String::from_utf8_lossy(stored);
        assert_eq!(status.uv.mode == ChannelMode::ManualOff, uv_off, "{label}");
    }
}

#[derive(Default)]
struct Warnings(Mutex<Vec<String>>);

struct Message<'a>(&'a mut String);

impl tracing::field::Visit for Message<'_> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            *self.0 = format!("{value:?}");
        }
    }
}

impl tracing::Subscriber for Warnings {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }

    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }

    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}

    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}

    fn event(&self, event: &tracing::Event<'_>) {
        if *event.metadata().level() == tracing::Level::WARN {
            let mut message = String::new();
            event.record(&mut Message(&mut message));
            self.0.lock().unwrap().push(message);
        }
    }

    fn enter(&self, _span: &tracing::span::Id) {}

    fn exit(&self, _span: &tracing::span::Id) {}
}

fn warnings_while(run: impl FnOnce()) -> Vec<String> {
    let warnings = Arc::new(Warnings::default());
    tracing::subscriber::with_default(Arc::clone(&warnings), run);
    let logged = warnings.0.lock().unwrap().clone();
    logged
}

#[test]
fn stored_settings_that_fail_validation_are_reported_before_the_fallback() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::Settings,
        br#"{"top_off":[],"water_change":[],"flow_lph":{},"resume_window_s":9999}"#.to_vec(),
    );

    let mut settings = None;
    let warnings = warnings_while(|| {
        settings = Some(service_on(&store, TestClock::synced(EPOCH)).settings());
    });

    assert_eq!(settings, Some(PumpSettings::DEFAULT));
    assert!(
        warnings.iter().any(|message| message.contains("invalid")),
        "{warnings:?}"
    );
}

#[test]
fn unavailable_supervision_refuses_new_water_movers() {
    let mut rig = Rig::new();
    rig.run(5, Some(&healthy()));
    assert_eq!(rig.service.status().supervision, Supervision::Off);

    rig.service.set_supervision(Supervision::Unavailable);
    let status = rig.service.status();
    assert_eq!(status.supervision, Supervision::Unavailable);
    assert_eq!(
        rig.service.pump_telemetry().supervision,
        Supervision::Unavailable
    );
    assert_eq!(
        rig.service.start_top_off(60),
        Err(StartError::Blocked(Blocker::SupervisionUnavailable))
    );
    assert_eq!(
        rig.service.start_water_change(PLAN),
        Err(StartError::Blocked(Blocker::SupervisionUnavailable))
    );
    rig.run(2, Some(&healthy()));
    assert_eq!(rig.pins(), [true, true, false, false]);

    rig.service.set_supervision(Supervision::Active);
    assert_eq!(rig.service.start_top_off(60), Ok(()));
}

#[test]
fn a_history_without_an_end_time_still_loads() {
    let store = Arc::new(VolatilePumpStore::default());
    store.put(
        PumpRecord::LastOperations,
        br#"{"top_off":{"kind":"top_off","started_at":5,"drain_ran_s":0,"settle_ran_s":0,"refill_ran_s":30,"result":"completed","resumed":false},"water_change":null}"#.to_vec(),
    );
    let service = service_on(&store, TestClock::synced(EPOCH));

    let record = service.status().last_top_off.expect("loaded");
    assert_eq!(record.refill_ran_s, 30);
    assert_eq!(record.finished_at, None);
}
