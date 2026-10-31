mod common;

use std::sync::Arc;

use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;
use aquarium_core::domain::metrics::TankTelemetryDto;
use aquarium_core::ports::outbound::TelemetrySink;

use common::{get, raw, start, TOKEN};

#[tokio::test]
async fn telemetry_with_a_valid_token_returns_the_snapshot() {
    LocalSet::new()
        .run_until(async {
            let store = Arc::new(InMemoryTelemetryStore::new());
            let mut snapshot = TankTelemetryDto::empty();
            snapshot.temperature.tank_celsius = Some(26.5);
            snapshot.system_health.uptime_seconds = 99;
            store.publish(snapshot);

            let addr = start(Arc::clone(&store)).await;
            let response = raw(addr, &get("/api/telemetry", Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            assert!(response.contains("application/json"), "got: {response}");
            assert!(
                response.contains("26.5"),
                "no reading in the body: {response}"
            );
            assert!(
                response.contains("\"uptime_seconds\":99"),
                "got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn health_with_a_valid_token_returns_system_health() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/health", Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            assert!(response.contains("free_heap_kb"), "got: {response}");
        })
        .await;
}
