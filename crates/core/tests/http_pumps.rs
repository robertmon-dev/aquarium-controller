#![recursion_limit = "256"]

mod common;

use std::sync::Arc;
use std::time::Duration;

use serde::de::DeserializeOwned;
use tokio::task::LocalSet;
use tokio::time::Instant;

use aquarium_core::domain::pump::{
    ChannelFault, ChannelMode, ChannelStatus, LevelConditions, OperationResult, OperationState,
    PumpId, PumpSettings, PumpsStatus,
};
use aquarium_core::domain::water::WaterLevelRole;
use aquarium_core::ports::inbound::PumpControl;

use common::{body, get, pumps, raw, start_with_pumps, PUMP_EPOCH, TOKEN};

const STATUS: &str = "/api/pumps";
const SETTINGS: &str = "/api/pumps/settings";
const TOP_OFF: &str = "/api/pumps/top-off";
const WATER_CHANGE: &str = "/api/water-change";

const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    reservoir_ready: true,
    tank_overflow: false,
    sump_full: false,
    sump_high: false,
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
                body("DELETE", "/api/flood/alarm", None, ""),
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

#[tokio::test]
async fn the_refill_stop_sensor_is_set_kept_and_cleared_over_the_api() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_pumps(pumps()).await;
            let patch = |payload: &'static str| body("PATCH", SETTINGS, Some(TOKEN), payload);

            let set: PumpSettings = parse(
                &raw(
                    addr,
                    &patch(
                        r#"{"refill_stop_sensor":"sump_biological","min_water_change_gap_h":24}"#,
                    ),
                )
                .await,
            );
            assert_eq!(set.refill_stop_sensor, Some(WaterLevelRole::SumpBiological));
            assert_eq!(set.min_water_change_gap_h, 24);

            let kept: PumpSettings = parse(&raw(addr, &patch(r#"{"resume_window_s":60}"#)).await);
            assert_eq!(
                kept.refill_stop_sensor,
                Some(WaterLevelRole::SumpBiological)
            );

            let cleared: PumpSettings =
                parse(&raw(addr, &patch(r#"{"refill_stop_sensor":null}"#)).await);
            assert_eq!(cleared.refill_stop_sensor, None);

            let wrong_sensor = raw(addr, &patch(r#"{"refill_stop_sensor":"main_tank"}"#)).await;
            assert_status(&wrong_sensor, 400);

            let long_gap = raw(addr, &patch(r#"{"min_water_change_gap_h":169}"#)).await;
            assert_status(&long_gap, 400);
        })
        .await;
}

#[tokio::test]
async fn the_reservoir_volume_bounds_a_water_change_over_the_api() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            pumps.observe(Some(HEALTHY), Duration::ZERO, Instant::now());
            let patch = |payload: &'static str| body("PATCH", SETTINGS, Some(TOKEN), payload);

            let set: PumpSettings = parse(
                &raw(
                    addr,
                    &patch(r#"{"flow_lph":{"makeup":600,"drain":1500},"reservoir_ready_l":50}"#),
                )
                .await,
            );
            assert_eq!(set.reservoir_ready_l, Some(50));

            let kept: PumpSettings = parse(&raw(addr, &patch(r#"{"resume_window_s":60}"#)).await);
            assert_eq!(kept.reservoir_ready_l, Some(50));

            assert_status(&raw(addr, &patch(r#"{"reservoir_ready_l":0}"#)).await, 400);
            let window = raw(
                addr,
                &patch(
                    r#"{"water_change":[{"cron":"0 10 * * sun","drain_s":121,"refill_s":300}]}"#,
                ),
            )
            .await;
            assert_status(&window, 400);
            assert!(
                window.contains("a water-change window drains more than reservoir_ready_l holds"),
                "got: {window}"
            );

            let refused = raw(
                addr,
                &body(
                    "POST",
                    WATER_CHANGE,
                    Some(TOKEN),
                    r#"{"drain_s":121,"refill_s":300}"#,
                ),
            )
            .await;
            assert_status(&refused, 400);
            assert!(
                refused.contains("the planned drain exceeds the water the reservoir holds"),
                "got: {refused}"
            );
            assert_eq!(pumps.status().operation.state, OperationState::Idle);

            let cleared: PumpSettings =
                parse(&raw(addr, &patch(r#"{"reservoir_ready_l":null}"#)).await);
            assert_eq!(cleared.reservoir_ready_l, None);
        })
        .await;
}

#[tokio::test]
async fn refill_blockers_explain_themselves() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            pumps.observe(
                Some(LevelConditions {
                    sump_full: true,
                    ..HEALTHY
                }),
                Duration::ZERO,
                Instant::now(),
            );

            let full = raw(
                addr,
                &body("POST", TOP_OFF, Some(TOKEN), r#"{"duration_s":60}"#),
            )
            .await;
            assert_status(&full, 423);
            assert!(full.contains("the sump is full"), "got: {full}");

            let water_change = body(
                "POST",
                WATER_CHANGE,
                Some(TOKEN),
                r#"{"drain_s":10,"refill_s":10}"#,
            );
            assert_status(&raw(addr, &water_change).await, 202);
            let levels = Some(LevelConditions {
                sump_full: true,
                ..HEALTHY
            });
            pumps.observe(levels, Duration::ZERO, Instant::now());
            pumps.record(PumpId::Drain, true, None, None);
            pumps.observe(levels, Duration::from_secs(1), Instant::now());
            assert_status(
                &raw(addr, &body("DELETE", WATER_CHANGE, Some(TOKEN), "")).await,
                204,
            );

            let too_soon = raw(addr, &water_change).await;
            assert_status(&too_soon, 423);
            assert!(
                too_soon.contains("the previous water change was too recent"),
                "got: {too_soon}"
            );
        })
        .await;
}

#[tokio::test]
async fn a_flooded_sump_or_tank_refuses_a_start() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            let water_change = body(
                "POST",
                WATER_CHANGE,
                Some(TOKEN),
                r#"{"drain_s":10,"refill_s":10}"#,
            );
            let top_off = body("POST", TOP_OFF, Some(TOKEN), r#"{"duration_s":60}"#);

            pumps.observe(
                Some(LevelConditions {
                    sump_high: true,
                    ..HEALTHY
                }),
                Duration::ZERO,
                Instant::now(),
            );
            for request in [&water_change, &top_off] {
                let refused = raw(addr, request).await;
                assert_status(&refused, 423);
                assert!(
                    refused.contains("a sump flood sensor is wet"),
                    "got: {refused}"
                );
            }

            pumps.observe(
                Some(LevelConditions {
                    tank_overflow: true,
                    ..HEALTHY
                }),
                Duration::ZERO,
                Instant::now(),
            );
            let refused = raw(addr, &water_change).await;
            assert_status(&refused, 423);
            assert!(
                refused.contains("the tank overflow sensor is wet"),
                "got: {refused}"
            );

            let status: PumpsStatus = parse(&raw(addr, &get(STATUS, Some(TOKEN))).await);
            assert!(!status.level_sensors_suspect);
        })
        .await;
}

#[tokio::test]
async fn a_water_change_is_refused_while_the_reservoir_is_low() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            pumps.observe(
                Some(LevelConditions {
                    reservoir_ready: false,
                    ..HEALTHY
                }),
                Duration::ZERO,
                Instant::now(),
            );

            let low = raw(
                addr,
                &body(
                    "POST",
                    WATER_CHANGE,
                    Some(TOKEN),
                    r#"{"drain_s":10,"refill_s":10}"#,
                ),
            )
            .await;
            assert_status(&low, 423);
            assert!(
                low.contains("the makeup reservoir does not hold enough for a water change"),
                "got: {low}"
            );

            let top_off = raw(
                addr,
                &body("POST", TOP_OFF, Some(TOKEN), r#"{"duration_s":60}"#),
            )
            .await;
            assert_status(&top_off, 202);
        })
        .await;
}

#[tokio::test]
async fn top_off_and_water_change_answer_only_to_the_barrel_flags() {
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            let top_off = body("POST", TOP_OFF, Some(TOKEN), r#"{"duration_s":60}"#);
            let water_change = body(
                "POST",
                WATER_CHANGE,
                Some(TOKEN),
                r#"{"drain_s":10,"refill_s":10}"#,
            );

            pumps.observe(
                Some(LevelConditions {
                    reservoir_empty: true,
                    reservoir_ready: false,
                    ..HEALTHY
                }),
                Duration::ZERO,
                Instant::now(),
            );
            let empty = raw(addr, &top_off).await;
            assert_status(&empty, 423);
            assert!(
                empty.contains("the makeup reservoir is empty"),
                "got: {empty}"
            );

            pumps.observe(
                Some(LevelConditions {
                    reservoir_ready: false,
                    ..HEALTHY
                }),
                Duration::ZERO,
                Instant::now(),
            );
            let low = raw(addr, &water_change).await;
            assert_status(&low, 423);
            assert!(
                low.contains("the makeup reservoir does not hold enough for a water change"),
                "got: {low}"
            );

            assert_status(&raw(addr, &top_off).await, 202);
        })
        .await;
}

#[tokio::test]
async fn flood_acknowledgement_requires_both_probes_dry() {
    use aquarium_core::domain::flood::FloodProbes;
    LocalSet::new()
        .run_until(async {
            let pumps = pumps();
            let addr = start_with_pumps(Arc::clone(&pumps)).await;
            for (reservoir, sump, code) in
                [(true, true, 423), (false, true, 423), (false, false, 204)]
            {
                pumps.observe_flood(FloodProbes { reservoir, sump });
                assert_status(
                    &raw(addr, &body("DELETE", "/api/flood/alarm", Some(TOKEN), "")).await,
                    code,
                );
            }
        })
        .await;
}
