#![recursion_limit = "256"]

mod common;

use std::net::SocketAddr;
use std::sync::Arc;

use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::{InMemoryTelemetryStore, VolatileCalibrationStore};
use aquarium_core::application::CalibrationService;
use aquarium_core::domain::calibration::Calibration;

use common::{
    body, calibration_defaults, get, raw, start_with_calibration, TestCalibration, TOKEN,
};

const PATH: &str = "/api/calibration";

fn service() -> Arc<TestCalibration> {
    Arc::new(CalibrationService::new(
        calibration_defaults(),
        VolatileCalibrationStore,
    ))
}

async fn serve(calibration: Arc<TestCalibration>) -> SocketAddr {
    start_with_calibration(
        Arc::new(InMemoryTelemetryStore::new()),
        common::lighting(),
        calibration,
    )
    .await
}

fn parse(response: &str) -> Calibration {
    let (head, payload) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no body in: {response}"));

    assert!(head.starts_with("HTTP/1.1 200"), "got: {response}");

    serde_json::from_str(payload.trim())
        .unwrap_or_else(|error| panic!("body is not a Calibration ({error}): {payload}"))
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "expected ~{expected}, got {actual}"
    );
}

#[tokio::test]
async fn the_current_calibration_is_served() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;
            let served = parse(&raw(addr, &get(PATH, Some(TOKEN))).await);

            assert_eq!(served, calibration_defaults());
        })
        .await;
}

#[tokio::test]
async fn reading_the_calibration_requires_the_token() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;
            let response = raw(addr, &get(PATH, None)).await;

            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn patching_one_field_leaves_the_rest_alone() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;

            let patched = parse(
                &raw(
                    addr,
                    &body(
                        "PATCH",
                        PATH,
                        Some(TOKEN),
                        r#"{"makeup_tank_celsius":22.5}"#,
                    ),
                )
                .await,
            );

            assert_close(patched.makeup_tank_celsius, 22.5);

            let read = parse(&raw(addr, &get(PATH, Some(TOKEN))).await);

            assert_close(read.makeup_tank_celsius, 22.5);
            assert_eq!(
                read.ph,
                calibration_defaults().ph,
                "the pH points must survive a patch that does not mention them"
            );
            assert_close(read.tds_factor, calibration_defaults().tds_factor);
        })
        .await;
}

#[tokio::test]
async fn patching_the_ph_points_replaces_all_three() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;

            let patched = parse(
                &raw(
                    addr,
                    &body(
                        "PATCH",
                        PATH,
                        Some(TOKEN),
                        r#"{"ph":{"neutral_volts":1.48,"acid_volts":1.66,"calibrated_at_celsius":21.0}}"#,
                    ),
                )
                .await,
            );

            assert_close(patched.ph.neutral_volts, 1.48);
            assert_close(patched.ph.acid_volts, 1.66);
            assert_close(patched.ph.calibrated_at_celsius, 21.0);
        })
        .await;
}

#[tokio::test]
async fn a_dead_electrode_calibration_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;

            let rejected = raw(
                addr,
                &body(
                    "PATCH",
                    PATH,
                    Some(TOKEN),
                    r#"{"ph":{"neutral_volts":1.5,"acid_volts":1.5,"calibrated_at_celsius":25.0}}"#,
                ),
            )
            .await;

            assert!(rejected.starts_with("HTTP/1.1 400"), "got: {rejected}");
        })
        .await;
}

#[tokio::test]
async fn a_rejected_patch_leaves_the_calibration_untouched() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;

            let rejected = raw(
                addr,
                &body(
                    "PATCH",
                    PATH,
                    Some(TOKEN),
                    r#"{"makeup_tank_celsius":80.0}"#,
                ),
            )
            .await;
            assert!(rejected.starts_with("HTTP/1.1 400"), "got: {rejected}");

            let read = parse(&raw(addr, &get(PATH, Some(TOKEN))).await);
            assert_eq!(read, calibration_defaults());
        })
        .await;
}

#[tokio::test]
async fn an_out_of_band_tds_factor_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;

            let rejected = raw(
                addr,
                &body("PATCH", PATH, Some(TOKEN), r#"{"tds_factor":50.0}"#),
            )
            .await;

            assert!(rejected.starts_with("HTTP/1.1 400"), "got: {rejected}");
        })
        .await;
}

#[tokio::test]
async fn an_empty_patch_is_accepted_and_changes_nothing() {
    LocalSet::new()
        .run_until(async {
            let addr = serve(service()).await;
            let response = parse(&raw(addr, &body("PATCH", PATH, Some(TOKEN), "{}")).await);

            assert_eq!(response, calibration_defaults());
        })
        .await;
}
