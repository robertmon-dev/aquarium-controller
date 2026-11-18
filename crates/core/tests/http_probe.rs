#![recursion_limit = "256"]

mod common;

use std::sync::Arc;

use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;
use aquarium_core::domain::metrics::ProbeVoltagesDto;
use aquarium_core::ports::outbound::ProbeVoltageSink;

use common::{get, raw, start, TOKEN};

const PATH: &str = "/api/probe";

fn parse(response: &str) -> ProbeVoltagesDto {
    let (head, payload) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no body in: {response}"));

    assert!(head.starts_with("HTTP/1.1 200"), "got: {response}");

    serde_json::from_str(payload.trim())
        .unwrap_or_else(|error| panic!("body is not ProbeVoltagesDto ({error}): {payload}"))
}

#[tokio::test]
async fn an_unsampled_device_reports_a_zero_timestamp_rather_than_failing() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let probes = parse(&raw(addr, &get(PATH, Some(TOKEN))).await);

            assert_eq!(probes, ProbeVoltagesDto::NEVER_SAMPLED);
        })
        .await;
}

#[tokio::test]
async fn the_last_sampled_voltages_are_served() {
    LocalSet::new()
        .run_until(async {
            let store = Arc::new(InMemoryTelemetryStore::new());
            store.publish_volts(Some(1.52), Some(0.91), 1_750_000_000);

            let addr = start(Arc::clone(&store)).await;
            let probes = parse(&raw(addr, &get(PATH, Some(TOKEN))).await);

            assert!((probes.ph.expect("a pH voltage") - 1.52).abs() < 1e-4);
            assert!((probes.tds.expect("a TDS voltage") - 0.91).abs() < 1e-4);
            assert_eq!(probes.sampled_at_epoch_s, 1_750_000_000);
        })
        .await;
}

#[tokio::test]
async fn a_disconnected_probe_is_served_as_null_not_as_zero() {
    LocalSet::new()
        .run_until(async {
            let store = Arc::new(InMemoryTelemetryStore::new());
            store.publish_volts(None, Some(0.91), 1_750_000_000);

            let addr = start(Arc::clone(&store)).await;
            let response = raw(addr, &get(PATH, Some(TOKEN))).await;

            assert!(
                response.contains("\"ph\":null"),
                "zero volts is a reading, absence is not: {response}"
            );
            assert_eq!(parse(&response).ph, None);
        })
        .await;
}

#[tokio::test]
async fn reading_the_probe_voltages_requires_the_token() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get(PATH, None)).await;

            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}
