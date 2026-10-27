use std::sync::Arc;

use tokio::sync::watch;

use crate::domain::metrics::TankTelemetryDto;
use crate::ports::inbound::TelemetryQuery;
use crate::ports::outbound::TelemetrySink;

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
}
