use std::sync::{Arc, Mutex};

use tokio::sync::{watch, Notify};
use tokio::time::Instant;

use crate::domain::light::{LightingOverride, LightingTarget};
use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::{LightingControl, TelemetryQuery};
use crate::ports::outbound::{LightingOverrideSource, TelemetrySink};

pub struct InMemoryTelemetryStore {
    tx: watch::Sender<TankTelemetryDto>,
}

impl InMemoryTelemetryStore {
    pub fn new() -> Self {
        Self {
            tx: watch::channel(TankTelemetryDto::empty()).0,
        }
    }
}

impl Default for InMemoryTelemetryStore {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetrySink for InMemoryTelemetryStore {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.tx.send_replace(snapshot);
    }
}

impl TelemetryQuery for InMemoryTelemetryStore {
    fn snapshot(&self) -> TankTelemetryDto {
        self.tx.borrow().clone()
    }

    fn subscribe(&self) -> watch::Receiver<TankTelemetryDto> {
        self.tx.subscribe()
    }
}

impl TelemetrySink for Arc<InMemoryTelemetryStore> {
    fn publish(&self, snapshot: TankTelemetryDto) {
        self.as_ref().publish(snapshot);
    }
}

struct StoredOverride {
    target: LightingTarget,
    expires_at: Option<Instant>,
}

pub struct InMemoryLightingStore {
    state: Mutex<Option<StoredOverride>>,
    notify: Arc<Notify>,
}

impl InMemoryLightingStore {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(None),
            notify: Arc::new(Notify::new()),
        }
    }

    pub fn change_signal(&self) -> Arc<Notify> {
        Arc::clone(&self.notify)
    }

    fn read(&self) -> Option<LightingOverride> {
        let guard = self.state.lock().expect("mutex poisoned by a panic");
        let stored = guard.as_ref()?;

        let expires_in_s = match stored.expires_at {
            None => None,
            Some(at) => {
                let now = Instant::now();
                if now >= at {
                    return None;
                }
                Some(at.saturating_duration_since(now).as_secs() as u32)
            }
        };

        Some(LightingOverride {
            target: stored.target,
            expires_in_s,
        })
    }
}

impl Default for InMemoryLightingStore {
    fn default() -> Self {
        Self::new()
    }
}

impl LightingControl for InMemoryLightingStore {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        let expires_at =
            duration_s.map(|s| Instant::now() + std::time::Duration::from_secs(s as u64));

        *self.state.lock().expect("mutex poisoned by a panic") =
            Some(StoredOverride { target, expires_at });

        self.notify.notify_one();
    }

    fn clear_override(&self) {
        *self.state.lock().expect("mutex poisoned by a panic") = None;
        self.notify.notify_one();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingOverrideSource for InMemoryLightingStore {
    fn active_override(&self) -> Option<LightingOverride> {
        self.read()
    }
}

impl LightingControl for Arc<InMemoryLightingStore> {
    fn set_override(&self, target: LightingTarget, duration_s: Option<u32>) {
        self.as_ref().set_override(target, duration_s);
    }

    fn clear_override(&self) {
        self.as_ref().clear_override();
    }

    fn current_override(&self) -> Option<LightingOverride> {
        self.as_ref().current_override()
    }
}

impl LightingOverrideSource for Arc<InMemoryLightingStore> {
    fn active_override(&self) -> Option<LightingOverride> {
        self.as_ref().active_override()
    }
}

#[cfg(test)]
mod tests {
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
            .expect("kanał musi żyć, dopóki store żyje");
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
}
