mod common;

use std::sync::Arc;

use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;

use common::{body, get, raw, start, OVERRIDE_PATH, TOKEN};

#[tokio::test]
async fn telemetry_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/telemetry", None)).await;
            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn telemetry_with_a_wrong_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/telemetry", Some("wrong"))).await;
            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn query_token_is_rejected_outside_the_events_route() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &format!(
                    "GET /api/telemetry?token={TOKEN} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                ),
            )
            .await;

            assert!(
                response.starts_with("HTTP/1.1 401"),
                "the query-token fallback must apply ONLY to /api/events, got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn events_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/events", None)).await;
            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn setting_an_override_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    None,
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;
            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn a_query_token_does_not_open_the_override_route() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    &format!("{OVERRIDE_PATH}?token={TOKEN}"),
                    None,
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 401"),
                "the query-token fallback must apply ONLY to /api/events, got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn unknown_path_with_a_valid_token_is_not_found() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/nope", Some(TOKEN))).await;
            assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
        })
        .await;
}
