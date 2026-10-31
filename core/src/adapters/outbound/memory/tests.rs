use super::InMemoryTelemetryStore;
use crate::domain::metrics::{SystemMode, TankTelemetryDto};
use crate::ports::inbound::TelemetryQuery;
use crate::ports::outbound::TelemetrySink;

fn snapshot_with_uptime(uptime: u64) -> TankTelemetryDto {
    let mut s = TankTelemetryDto::empty();
    s.system_health.uptime_seconds = uptime;
    s
}

#[test]
fn serves_initial_snapshot_before_first_measurement() {
    let store = InMemoryTelemetryStore::new();
    let s = store.snapshot();
    assert_eq!(s.system_mode, SystemMode::Normal);
    assert_eq!(s.temperature.tank_celsius, None);
    assert!(!s.temperature.sensors_healthy);
}

#[test]
fn publish_replaces_the_snapshot() {
    let store = InMemoryTelemetryStore::new();
    store.publish(snapshot_with_uptime(42));
    assert_eq!(store.snapshot().system_health.uptime_seconds, 42);
}

#[tokio::test]
async fn subscriber_is_woken_by_publish() {
    let store = InMemoryTelemetryStore::new();
    let mut rx = store.subscribe();

    store.publish(snapshot_with_uptime(7));

    rx.changed()
        .await
        .expect("the channel must live as long as the store does");
    assert_eq!(rx.borrow().system_health.uptime_seconds, 7);
}

#[tokio::test]
async fn slow_subscriber_sees_only_the_latest_value() {
    let store = InMemoryTelemetryStore::new();
    let mut rx = store.subscribe();

    store.publish(snapshot_with_uptime(1));
    store.publish(snapshot_with_uptime(2));
    store.publish(snapshot_with_uptime(3));

    rx.changed().await.unwrap();
    assert_eq!(rx.borrow().system_health.uptime_seconds, 3);
}

#[test]
fn arc_wrapper_forwards_publish() {
    let store = std::sync::Arc::new(InMemoryTelemetryStore::new());
    TelemetrySink::publish(&store, snapshot_with_uptime(5));
    assert_eq!(store.snapshot().system_health.uptime_seconds, 5);
}

#[test]
fn publishing_without_subscribers_does_not_panic() {
    let store = InMemoryTelemetryStore::new();
    store.publish(snapshot_with_uptime(1));
    assert_eq!(store.snapshot().system_health.uptime_seconds, 1);
}

mod lighting {
    use std::time::Duration;

    use super::super::InMemoryLightingStore;
    use crate::domain::light::{ChannelLevels, LightingTarget};
    use crate::domain::metrics::LightingProfile;
    use crate::ports::inbound::LightingControl;
    use crate::ports::outbound::LightingOverrideSource;

    fn moonlight() -> LightingTarget {
        LightingTarget::Profile(LightingProfile::Moonlight)
    }

    #[test]
    fn starts_with_no_override() {
        let store = InMemoryLightingStore::new();
        assert!(store.current_override().is_none());
        assert!(store.active_override().is_none());
    }

    #[test]
    fn set_then_read_returns_the_target() {
        let store = InMemoryLightingStore::new();
        store.set_override(moonlight(), None);

        let over = store.current_override().expect("override was just set");
        assert_eq!(over.target, moonlight());
        assert_eq!(
            over.expires_in_s, None,
            "a missing duration_s means open-ended"
        );
    }

    #[test]
    fn setting_twice_replaces_rather_than_stacks() {
        let store = InMemoryLightingStore::new();
        store.set_override(moonlight(), None);
        store.set_override(LightingTarget::Channels(ChannelLevels::new(80, 20)), None);

        let over = store.current_override().expect("an override exists");
        assert_eq!(
            over.target,
            LightingTarget::Channels(ChannelLevels::new(80, 20))
        );
    }

    #[test]
    fn clear_removes_the_override() {
        let store = InMemoryLightingStore::new();
        store.set_override(moonlight(), None);
        store.clear_override();
        assert!(store.current_override().is_none());
    }

    #[test]
    fn clearing_nothing_does_not_panic() {
        let store = InMemoryLightingStore::new();
        store.clear_override();
        assert!(store.current_override().is_none());
    }

    #[tokio::test(start_paused = true)]
    async fn a_timed_override_expires_on_its_own() {
        let store = InMemoryLightingStore::new();
        store.set_override(moonlight(), Some(60));

        assert!(
            store.current_override().is_some(),
            "alive right after being set"
        );

        tokio::time::advance(Duration::from_secs(61)).await;

        assert!(
            store.current_override().is_none(),
            "once the TTL elapses the override MUST disappear on its own"
        );
        assert!(
            store.active_override().is_none(),
            "the collector and HTTP must see exactly the same state"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn remaining_time_counts_down() {
        let store = InMemoryLightingStore::new();
        store.set_override(moonlight(), Some(100));

        tokio::time::advance(Duration::from_secs(40)).await;

        let remaining = store
            .current_override()
            .expect("not expired yet")
            .expires_in_s
            .expect("a timed override reports its remaining time");

        assert!(
            (55..=60).contains(&remaining),
            "expected ~60 s, got {remaining}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn an_open_ended_override_never_expires() {
        let store = InMemoryLightingStore::new();
        store.set_override(moonlight(), None);

        tokio::time::advance(Duration::from_secs(86_400)).await;

        assert!(
            store.current_override().is_some(),
            "without a TTL the override survives a full day"
        );
    }

    #[tokio::test]
    async fn setting_an_override_wakes_the_collector() {
        let store = InMemoryLightingStore::new();
        let signal = store.change_signal();

        store.set_override(moonlight(), None);

        tokio::time::timeout(Duration::from_millis(100), signal.notified())
            .await
            .expect("notify_one leaves a permit, so waiting AFTER the signal still works");
    }

    #[tokio::test]
    async fn clearing_an_override_also_wakes_the_collector() {
        let store = InMemoryLightingStore::new();
        let signal = store.change_signal();

        store.clear_override();

        tokio::time::timeout(Duration::from_millis(100), signal.notified())
            .await
            .expect("returning to the schedule must refresh telemetry too");
    }
}
