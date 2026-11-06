#![recursion_limit = "256"]

mod common;

use std::sync::Arc;

use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;

use common::{get, preflight, raw, start, ORIGIN, TOKEN};

#[tokio::test]
async fn a_preflight_is_answered_without_a_token() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &preflight("/api/health", "GET")).await;

            assert!(
                response.starts_with("HTTP/1.1 204"),
                "a preflight carries no credentials, so it must be answered before the token check, got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn a_preflight_allows_the_authorization_header() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &preflight("/api/telemetry", "GET"))
                .await
                .to_lowercase();

            assert!(
                response.contains("access-control-allow-headers: authorization"),
                "without this the browser never sends the bearer token, got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn a_preflight_allows_every_method_the_api_serves() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &preflight("/api/lighting/override", "PUT"))
                .await
                .to_lowercase();

            for method in ["get", "put", "delete"] {
                assert!(
                    response.contains(method),
                    "allow-methods must cover {method}, got: {response}"
                );
            }
        })
        .await;
}

#[tokio::test]
async fn an_authorized_response_carries_the_allowed_origin() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/health", Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            assert!(
                response
                    .to_lowercase()
                    .contains(&format!("access-control-allow-origin: {ORIGIN}")),
                "got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn a_rejected_response_also_carries_the_allowed_origin() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/health", None)).await;

            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
            assert!(
                response
                    .to_lowercase()
                    .contains("access-control-allow-origin"),
                "without the header on the 401 the browser hands the app an opaque failure \
                 instead of a readable rejection, got: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn the_preflight_shortcut_does_not_open_the_api() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/telemetry", None)).await;

            assert!(
                response.starts_with("HTTP/1.1 401"),
                "answering OPTIONS before the token check must not weaken any other method, got: {response}"
            );
        })
        .await;
}
