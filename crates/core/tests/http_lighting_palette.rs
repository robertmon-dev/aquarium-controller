#![recursion_limit = "256"]

mod common;

use std::sync::Arc;

use serde_json::Value;
use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;
use aquarium_core::domain::lighting::LightingPalette;

use common::{body, get, lighting, raw, start_with_lighting, TOKEN};

const PALETTE: &str = "/api/lighting/palette";

fn json(response: &str) -> Value {
    let (_, payload) = response.split_once("\r\n\r\n").expect("headers end");
    serde_json::from_str(payload).expect("json body")
}

#[tokio::test]
async fn the_default_palette_is_served() {
    LocalSet::new()
        .run_until(async {
            let addr =
                start_with_lighting(Arc::new(InMemoryTelemetryStore::new()), lighting()).await;
            let response = raw(addr, &get(PALETTE, Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            let palette: LightingPalette = serde_json::from_value(json(&response)).unwrap();
            assert_eq!(palette, LightingPalette::DEFAULT);
        })
        .await;
}

#[tokio::test]
async fn a_patch_changes_only_the_given_colour() {
    LocalSet::new()
        .run_until(async {
            let service = lighting();
            let addr = start_with_lighting(
                Arc::new(InMemoryTelemetryStore::new()),
                Arc::clone(&service),
            )
            .await;
            let response = raw(
                addr,
                &body(
                    "PATCH",
                    PALETTE,
                    Some(TOKEN),
                    r#"{"moonlight":{"red_pct":6,"green_pct":0,"blue_pct":12,"white_pct":0}}"#,
                ),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            let palette: LightingPalette = serde_json::from_value(json(&response)).unwrap();
            assert_eq!(palette.moonlight.blue_pct, 12);
            assert_eq!(palette.daylight, LightingPalette::DEFAULT.daylight);
        })
        .await;
}

#[tokio::test]
async fn an_out_of_range_colour_is_a_bad_request() {
    LocalSet::new()
        .run_until(async {
            let addr =
                start_with_lighting(Arc::new(InMemoryTelemetryStore::new()), lighting()).await;
            let response = raw(
                addr,
                &body(
                    "PATCH",
                    PALETTE,
                    Some(TOKEN),
                    r#"{"daylight":{"red_pct":0,"green_pct":0,"blue_pct":0,"white_pct":150}}"#,
                ),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn the_schedule_preview_follows_the_stored_palette() {
    LocalSet::new()
        .run_until(async {
            let addr =
                start_with_lighting(Arc::new(InMemoryTelemetryStore::new()), lighting()).await;
            raw(
                addr,
                &body(
                    "PATCH",
                    PALETTE,
                    Some(TOKEN),
                    r#"{"daylight":{"red_pct":1,"green_pct":2,"blue_pct":3,"white_pct":4}}"#,
                ),
            )
            .await;

            let response = raw(addr, &get("/api/lighting/schedule/6", Some(TOKEN))).await;
            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");

            let preview = json(&response);
            let noon = 12 * 60 / 5;
            assert_eq!(preview["white_pct"][noon], 4);
            assert_eq!(preview["red_pct"][noon], 1);
        })
        .await;
}
