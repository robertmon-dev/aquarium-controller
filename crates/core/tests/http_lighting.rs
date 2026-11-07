mod common;

use std::sync::Arc;

use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::{InMemoryLightingStore, InMemoryTelemetryStore};

use common::{body, get, raw, start, start_with_lighting, OVERRIDE_PATH, TOKEN};

#[tokio::test]
async fn no_override_set_yields_no_content() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;
            assert!(response.starts_with("HTTP/1.1 204"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn setting_a_profile_override_is_reflected_in_telemetry() {
    LocalSet::new()
        .run_until(async {
            let telemetry = Arc::new(InMemoryTelemetryStore::new());
            let lighting = Arc::new(InMemoryLightingStore::new());
            let addr = start_with_lighting(Arc::clone(&telemetry), Arc::clone(&lighting)).await;

            let set = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"profile":"moonlight"}}"#,
                ),
            )
            .await;
            assert!(set.starts_with("HTTP/1.1 200"), "got: {set}");

            let read = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;
            assert!(read.starts_with("HTTP/1.1 200"), "got: {read}");
            assert!(read.contains("moonlight"), "got: {read}");
        })
        .await;
}

#[tokio::test]
async fn setting_raw_channels_round_trips() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;

            let set = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"channels":{"white_pct":80,"rgb_pct":20}}}"#,
                ),
            )
            .await;

            assert!(set.starts_with("HTTP/1.1 200"), "got: {set}");
            assert!(set.contains("\"white_pct\":80"), "got: {set}");
        })
        .await;
}

#[tokio::test]
async fn a_channel_level_above_one_hundred_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"channels":{"white_pct":150,"rgb_pct":20}}}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "u8 admits 150, so validation MUST catch it: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn a_zero_duration_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"profile":"off"},"duration_s":0}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 400"),
                "set-and-immediately-expire is a contradictory request: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn malformed_json_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &body("PUT", OVERRIDE_PATH, Some(TOKEN), "{nonsense")).await;
            assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn deleting_an_override_returns_to_the_schedule() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;

            let _ = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    Some(TOKEN),
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;

            let deleted = raw(addr, &body("DELETE", OVERRIDE_PATH, Some(TOKEN), "")).await;
            assert!(deleted.starts_with("HTTP/1.1 204"), "got: {deleted}");

            let read = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;
            assert!(read.starts_with("HTTP/1.1 204"), "got: {read}");
        })
        .await;
}

#[tokio::test]
async fn deleting_a_nonexistent_override_is_idempotent() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &body("DELETE", OVERRIDE_PATH, Some(TOKEN), "")).await;
            assert!(response.starts_with("HTTP/1.1 204"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn an_absent_override_sends_no_body() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get(OVERRIDE_PATH, Some(TOKEN))).await;

            let (_, body) = response.split_once("\r\n\r\n").expect("headers end");

            assert!(
                body.is_empty(),
                "a 204 must not carry a body - a client on a keep-alive connection reads \
                 these bytes as the start of the next response, got: {body:?}"
            );
        })
        .await;
}

#[tokio::test]
async fn deleting_an_override_sends_no_body() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &body("DELETE", OVERRIDE_PATH, Some(TOKEN), "")).await;

            let (_, payload) = response.split_once("\r\n\r\n").expect("headers end");

            assert!(
                payload.is_empty(),
                "a 204 must not carry a body, got: {payload:?}"
            );
        })
        .await;
}
