#![recursion_limit = "256"]

mod common;

use std::sync::Arc;

use tokio::task::LocalSet;
use tokio::time::Instant;

use aquarium_core::application::reservoir::Observation;
use aquarium_core::domain::pump::Supervision;
use aquarium_core::domain::reservoir::{ReservoirConditions, ReservoirSettings, ReservoirStatus};
use aquarium_core::ports::inbound::ReservoirControl;

use common::{
    assert_refused, body, delete_at, get, observe_barrel, parse_body, post_fill, raw, ready_barrel,
    refusing_reservoir, reservoir, start_with_reservoir, status_of, FAULT_LOCK, HALF_BARREL,
    REFUSED_WRITE, RESERVOIR_FAULT, RESERVOIR_FILL, TOKEN,
};

#[tokio::test]
async fn reservoir_routes_require_the_token() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_reservoir(reservoir()).await;
            for request in [
                get("/api/reservoir", None),
                get("/api/reservoir/settings", None),
                body("PATCH", "/api/reservoir/settings", None, "{}"),
                body("PUT", "/api/reservoir/ro/override", None, r#"{"on":false}"#),
                body("DELETE", "/api/reservoir/ro/override", None, ""),
                body("POST", "/api/reservoir/ro_valve/runtime/reset", None, ""),
                body("POST", RESERVOIR_FILL, None, ""),
                body("DELETE", RESERVOIR_FILL, None, ""),
                body("DELETE", RESERVOIR_FAULT, None, ""),
            ] {
                assert_eq!(status_of(&raw(addr, &request).await), 401);
            }
        })
        .await;
}

#[tokio::test]
async fn the_status_and_settings_are_served() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_reservoir(ready_barrel()).await;

            let status: ReservoirStatus =
                parse_body(&raw(addr, &get("/api/reservoir", Some(TOKEN))).await);
            assert_eq!(status.conditions, Some(HALF_BARREL));
            assert_eq!(status.supervision, Supervision::Active);
            assert!(status.fill.is_none());
            assert!(status.fault.is_none());

            let settings: ReservoirSettings =
                parse_body(&raw(addr, &get("/api/reservoir/settings", Some(TOKEN))).await);
            assert_eq!(settings, ReservoirSettings::DEFAULT);
        })
        .await;
}

#[tokio::test]
async fn a_manual_fill_is_accepted_once_and_cancelled() {
    LocalSet::new()
        .run_until(async {
            let reservoir = ready_barrel();
            let addr = start_with_reservoir(Arc::clone(&reservoir)).await;

            assert_eq!(status_of(&post_fill(addr).await), 202);
            assert!(reservoir.status().fill.is_some());
            assert_refused(
                &post_fill(addr).await,
                409,
                "a barrel fill is already running",
            );
            assert_eq!(status_of(&delete_at(addr, RESERVOIR_FILL).await), 204);
            assert_eq!(status_of(&delete_at(addr, RESERVOIR_FILL).await), 204);

            let status: ReservoirStatus =
                parse_body(&raw(addr, &get("/api/reservoir", Some(TOKEN))).await);
            assert!(status.fill.is_none());
        })
        .await;
}

#[tokio::test]
async fn every_refusal_of_a_manual_fill_has_its_code_and_reason() {
    LocalSet::new()
        .run_until(async {
            let unsupervised = reservoir();
            observe_barrel(&unsupervised, Some(HALF_BARREL), false);
            let addr = start_with_reservoir(unsupervised).await;
            assert_refused(
                &post_fill(addr).await,
                423,
                "the controller watchdog is not supervising the barrel",
            );

            let blind = reservoir();
            blind.set_supervision(Supervision::Active);
            let addr = start_with_reservoir(blind).await;
            assert_refused(
                &post_fill(addr).await,
                423,
                "barrel level readings are unavailable",
            );

            let leaking = ready_barrel();
            let wet = ReservoirConditions {
                leak: Some(true),
                ..HALF_BARREL
            };
            observe_barrel(&leaking, Some(wet), false);
            let addr = start_with_reservoir(leaking).await;
            assert_refused(&post_fill(addr).await, 423, FAULT_LOCK);
            assert_refused(
                &delete_at(addr, RESERVOIR_FAULT).await,
                423,
                "the leak sensor is wet",
            );

            let uncertain = ready_barrel();
            uncertain.observe(Observation {
                uncertain: true,
                ..Observation::idle(Some(HALF_BARREL), Instant::now())
            });
            let addr = start_with_reservoir(uncertain).await;
            assert_refused(
                &post_fill(addr).await,
                423,
                "the other inlet valve may still be open",
            );

            let full = ready_barrel();
            observe_barrel(
                &full,
                Some(ReservoirConditions {
                    full: true,
                    ..HALF_BARREL
                }),
                false,
            );
            let addr = start_with_reservoir(full).await;
            assert_refused(&post_fill(addr).await, 423, "the barrel is full");

            let changing = ready_barrel();
            observe_barrel(&changing, Some(HALF_BARREL), true);
            let addr = start_with_reservoir(changing).await;
            assert_refused(
                &post_fill(addr).await,
                423,
                "a water change is running; the barrel fills after it",
            );

            let targeted = reservoir();
            targeted.set_supervision(Supervision::Active);
            targeted
                .apply_settings(serde_json::from_str(r#"{"target_tds_ppm":150}"#).expect("patch"))
                .expect("target set");
            targeted.observe(Observation::idle(Some(HALF_BARREL), Instant::now()));
            let addr = start_with_reservoir(targeted).await;
            assert_refused(
                &post_fill(addr).await,
                423,
                "no fresh TDS reading for the target",
            );
        })
        .await;
}

#[tokio::test]
async fn a_full_barrel_is_reported_before_a_water_change() {
    LocalSet::new()
        .run_until(async {
            let reservoir = ready_barrel();
            observe_barrel(
                &reservoir,
                Some(ReservoirConditions {
                    full: true,
                    ..HALF_BARREL
                }),
                true,
            );
            let addr = start_with_reservoir(reservoir).await;
            assert_refused(&post_fill(addr).await, 423, "the barrel is full");
        })
        .await;
}

#[tokio::test]
async fn clearing_a_fault_needs_a_dry_leak_sensor() {
    LocalSet::new()
        .run_until(async {
            let reservoir = ready_barrel();
            let addr = start_with_reservoir(Arc::clone(&reservoir)).await;
            assert_eq!(status_of(&delete_at(addr, RESERVOIR_FAULT).await), 204);

            assert_eq!(status_of(&post_fill(addr).await), 202);
            let wet = ReservoirConditions {
                leak: Some(true),
                ..HALF_BARREL
            };
            observe_barrel(&reservoir, Some(wet), false);
            assert_refused(&post_fill(addr).await, 423, FAULT_LOCK);
            assert_refused(
                &delete_at(addr, RESERVOIR_FAULT).await,
                423,
                "the leak sensor is wet",
            );

            observe_barrel(&reservoir, Some(HALF_BARREL), false);
            assert_refused(&post_fill(addr).await, 423, FAULT_LOCK);
            assert_eq!(status_of(&delete_at(addr, RESERVOIR_FAULT).await), 204);
            assert!(reservoir.status().fault.is_none());
            assert_eq!(status_of(&post_fill(addr).await), 202);
        })
        .await;
}

#[tokio::test]
async fn settings_are_patched_and_validated() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_reservoir(reservoir()).await;
            let patch = |json: &str| body("PATCH", "/api/reservoir/settings", Some(TOKEN), json);

            let updated: ReservoirSettings =
                parse_body(&raw(addr, &patch(r#"{"auto_fill":true,"target_tds_ppm":150}"#)).await);
            assert!(updated.auto_fill);
            assert_eq!(updated.target_tds_ppm, Some(150));

            assert_refused(
                &raw(addr, &patch(r#"{"inlet_rest_s":5}"#)).await,
                400,
                "inlet_rest_s must be between 60 and 3600",
            );

            let cleared: ReservoirSettings =
                parse_body(&raw(addr, &patch(r#"{"target_tds_ppm":null}"#)).await);
            assert_eq!(cleared.target_tds_ppm, None);
            assert!(cleared.auto_fill);
        })
        .await;
}

#[tokio::test]
async fn storage_failures_answer_service_unavailable() {
    LocalSet::new()
        .run_until(async {
            let (reservoir, store) = refusing_reservoir();
            reservoir.set_supervision(Supervision::Active);
            observe_barrel(&reservoir, Some(HALF_BARREL), false);
            let addr = start_with_reservoir(Arc::clone(&reservoir)).await;
            let patch = body(
                "PATCH",
                "/api/reservoir/settings",
                Some(TOKEN),
                r#"{"auto_fill":true}"#,
            );

            store.refuse(true);
            assert_refused(&raw(addr, &patch).await, 503, REFUSED_WRITE);
            assert!(!reservoir.settings().auto_fill);
            assert_refused(
                &post_fill(addr).await,
                503,
                "the fill could not be recorded",
            );
            assert!(reservoir.status().fill.is_none());

            store.refuse(false);
            assert_eq!(status_of(&post_fill(addr).await), 202);
            store.refuse(true);
            assert_refused(
                &delete_at(addr, RESERVOIR_FILL).await,
                503,
                "cancelled, but not recorded; the controller keeps retrying",
            );
            assert!(reservoir.status().invalidation_pending);

            store.refuse(false);
            let wet = ReservoirConditions {
                leak: Some(true),
                ..HALF_BARREL
            };
            observe_barrel(&reservoir, Some(wet), false);
            observe_barrel(&reservoir, Some(HALF_BARREL), false);
            assert!(reservoir.status().fault.is_some());
            store.refuse(true);
            assert_refused(
                &delete_at(addr, RESERVOIR_FAULT).await,
                503,
                "the fault could not be cleared in storage",
            );
            assert!(reservoir.status().fault.is_some());

            store.refuse(false);
            assert_eq!(status_of(&delete_at(addr, RESERVOIR_FAULT).await), 204);
        })
        .await;
}
