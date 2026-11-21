#![recursion_limit = "256"]

mod common;

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::task::LocalSet;
use tokio::time::Instant;

use aquarium_core::domain::pump::{
    ChannelFault, ChannelMode, ChannelStatus, LevelConditions, OperationResult, OperationState,
    PumpSettings, PumpsStatus,
};
use aquarium_core::ports::inbound::PumpControl;

use common::{body, get, pumps, raw, start_with_pumps, PUMP_EPOCH, TOKEN};

const STATUS: &str = "/api/pumps";
const SETTINGS: &str = "/api/pumps/settings";
const TOP_OFF: &str = "/api/pumps/top-off";
const WATER_CHANGE: &str = "/api/water-change";

const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    tank_overflow: false,
};

fn parse<T: DeserializeOwned>(response: &str) -> T {
    let (head, payload) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no body in: {response}"));

    assert!(head.starts_with("HTTP/1.1 200"), "got: {response}");

    serde_json::from_str(payload.trim())
        .unwrap_or_else(|error| panic!("unexpected body ({error}): {payload}"))
}

fn assert_status(response: &str, code: u16) {
    assert!(
        response.starts_with(&format!("HTTP/1.1 {code}")),
        "expected {code}, got: {response}"
    );
}

#[tokio::test]
async fn pump_routes_require_the_token() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_pumps(pumps()).await;

            for request in [
                get(STATUS, None),
                get(SETTINGS, None),
                body("PATCH", SETTINGS, None, "{}"),
                body("PUT", "/api/pumps/uv/override", None, "{}"),
                body("DELETE", "/api/pumps/uv/override", None, ""),
                body("POST", "/api/pumps/uv/runtime/reset", None, ""),
                body("POST", TOP_OFF, None, r#"{"duration_s":60}"#),
                body("POST", "/api/pumps/top-off/resume", None, ""),
                body("DELETE", TOP_OFF, None, ""),
                body("POST", WATER_CHANGE, None, r#"{"drain_s":1,"refill_s":1}"#),
                body("POST", "/api/water-change/resume", None, ""),
                body("DELETE", WATER_CHANGE, None, ""),
            ] {
                assert_status(&raw(addr, &request).await, 401);
            }
        })
        .await;
}

#[tokio::test]
async fn the_status_starts_unavailable_and_idle() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_pumps(pumps()).await;

            let status: PumpsStatus = parse(&raw(addr, &get(STATUS, Some(TOKEN))).await);

            assert_eq!(
                status.channels.return_pump.fault,
                Some(ChannelFault::Unavailable)
            );
            assert_eq!(status.operation.state, OperationState::Idle);
        })
        .await;
}

#[tokio::test]
async fn settings_are_served_patched_and_validated() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_pumps(pumps()).await;

            let served: PumpSettings = parse(&raw(addr, &get(SETTINGS, Some(TOKEN))).await);
            assert_eq!(served, PumpSettings::DEFAULT);

            let patched: PumpSettings = parse(
                &raw(
                    addr,
                    &body(
                        "PATCH",
                        SETTINGS,
                        Some(TOKEN),
                        r#"{"top_off":[{"cron":"0 8 * * *","duration_s":120}],"flow_lph":{"makeup":300,"drain":null}}"#,
                    ),
                )
                .await,
            );
            assert_eq!(patched.top_off.len(), 1);
            assert_eq!(patched.flow_lph.makeup, Some(300));

            let rejected = raw(
                addr,
                &body(
                    "PATCH",
                    SETTINGS,
                    Some(TOKEN),
                    r#"{"water_change":[{"cron":"0 25 * * *","drain_s":10,"refill_s":10}]}"#,
                ),
            )
            .await;
            assert_status(&rejected, 400);
            assert!(rejected.contains("cron value out of range"), "got: {rejected}");
        })
        .await;
}

#[tokio::test]
async fn overrides_follow_the_channel_rules() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_pumps(pumps()).await;

            let missing = raw(
                addr,
                &body("PUT", "/api/pumps/return/override", Some(TOKEN), "{}"),
            )
            .await;
            assert_status(&missing, 400);

            let makeup = raw(
                addr,
                &body(
                    "PUT",
                    "/api/pumps/makeup/override",
                    Some(TOKEN),
                    r#"{"duration_s":600}"#,
                ),
            )
            .await;
            assert_status(&makeup, 404);

            let uv: ChannelStatus = parse(
                &raw(
                    addr,
                    &body(
                        "PUT",
                        "/api/pumps/uv/override",
                        Some(TOKEN),
                        r#"{"duration_s":null}"#,
                    ),
                )
                .await,
            );
            assert_eq!(uv.mode, ChannelMode::ManualOff);
            assert_eq!(uv.override_expires_in_s, None);

            assert_status(
                &raw(
                    addr,
                    &body("DELETE", "/api/pumps/uv/override", Some(TOKEN), ""),
                )
                .await,
                204,
            );
            assert_status(
                &raw(
                    addr,
                    &body("DELETE", "/api/pumps/drain/override", Some(TOKEN), ""),
                )
                .await,
                404,
            );
        })
        .await;
}

#[tokio::test]
async fn a_top_off_needs_levels_and_conflicts_with_itself() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            let start = body("POST", TOP_OFF, Some(TOKEN), r#"{"duration_s":60}"#);

            assert_status(&raw(addr, &start).await, 423);

            pumps.observe(Some(HEALTHY), Duration::ZERO, Instant::now());

            assert_status(&raw(addr, &start).await, 202);
            assert_status(&raw(addr, &start).await, 409);
            assert_status(
                &raw(
                    addr,
                    &body("POST", "/api/pumps/top-off/resume", Some(TOKEN), ""),
                )
                .await,
                409,
            );
            assert_status(
                &raw(addr, &body("DELETE", TOP_OFF, Some(TOKEN), "")).await,
                204,
            );

            assert_eq!(
                pumps.status().last_top_off.map(|record| record.result),
                Some(OperationResult::Aborted)
            );
        })
        .await;
}

#[tokio::test]
async fn a_water_change_is_validated_started_and_cancelled() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            pumps.observe(Some(HEALTHY), Duration::ZERO, Instant::now());

            let invalid = raw(
                addr,
                &body(
                    "POST",
                    WATER_CHANGE,
                    Some(TOKEN),
                    r#"{"drain_s":0,"refill_s":10}"#,
                ),
            )
            .await;
            assert_status(&invalid, 400);

            let started = raw(
                addr,
                &body(
                    "POST",
                    WATER_CHANGE,
                    Some(TOKEN),
                    r#"{"drain_s":10,"refill_s":10}"#,
                ),
            )
            .await;
            assert_status(&started, 202);
            assert_eq!(pumps.status().operation.state, OperationState::Draining);

            assert_status(
                &raw(addr, &body("DELETE", WATER_CHANGE, Some(TOKEN), "")).await,
                204,
            );
            assert_eq!(pumps.status().operation.state, OperationState::Idle);
        })
        .await;
}

#[tokio::test]
async fn a_runtime_counter_resets_over_the_api() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;

            assert_status(
                &raw(
                    addr,
                    &body("POST", "/api/pumps/uv/runtime/reset", Some(TOKEN), ""),
                )
                .await,
                204,
            );
            assert_eq!(
                pumps.status().channels.uv.runtime_reset_at,
                Some(PUMP_EPOCH)
            );
        })
        .await;
}
