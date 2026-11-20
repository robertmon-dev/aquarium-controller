#![recursion_limit = "256"]

mod common;

use serde::de::DeserializeOwned;
use tokio::task::LocalSet;

use aquarium_core::domain::heater::{
    HeaterFault, HeaterMode, HeaterSettings, HeaterStatus, HeatersStatus,
};

use common::{body, get, heater_defaults, heaters, raw, start_with_heaters, TOKEN};

const STATUS: &str = "/api/heaters";
const SETTINGS: &str = "/api/heaters/settings";
const TANK_OVERRIDE: &str = "/api/heaters/tank/override";

fn parse<T: DeserializeOwned>(response: &str) -> T {
    let (head, payload) = response
        .split_once("\r\n\r\n")
        .unwrap_or_else(|| panic!("no body in: {response}"));

    assert!(head.starts_with("HTTP/1.1 200"), "got: {response}");

    serde_json::from_str(payload.trim())
        .unwrap_or_else(|error| panic!("unexpected body ({error}): {payload}"))
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 1e-4,
        "expected ~{expected}, got {actual}"
    );
}

#[tokio::test]
async fn status_before_the_first_tick_is_unavailable() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            let status: HeatersStatus = parse(&raw(addr, &get(STATUS, Some(TOKEN))).await);

            assert_eq!(status.tank.fault, Some(HeaterFault::Unavailable));
            assert_close(status.tank.target_celsius, 27.0);
            assert_close(status.sump.target_celsius, 26.0);
        })
        .await;
}

#[tokio::test]
async fn heater_routes_require_the_token() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            for request in [
                get(STATUS, None),
                get(SETTINGS, None),
                body("PUT", TANK_OVERRIDE, None, r#"{"state":"off"}"#),
                body("PATCH", SETTINGS, None, r#"{"cutoff_celsius":29.5}"#),
                body("DELETE", TANK_OVERRIDE, None, ""),
            ] {
                let response = raw(addr, &request).await;
                assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
            }
        })
        .await;
}

#[tokio::test]
async fn settings_are_served_and_patched() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            let served: HeaterSettings = parse(&raw(addr, &get(SETTINGS, Some(TOKEN))).await);
            assert_eq!(served, heater_defaults());

            let patched: HeaterSettings = parse(
                &raw(
                    addr,
                    &body("PATCH", SETTINGS, Some(TOKEN), r#"{"cutoff_celsius":29.5}"#),
                )
                .await,
            );
            assert_close(patched.cutoff_celsius, 29.5);
            assert_eq!(patched.tank, heater_defaults().tank);

            let read: HeaterSettings = parse(&raw(addr, &get(SETTINGS, Some(TOKEN))).await);
            assert_close(read.cutoff_celsius, 29.5);
        })
        .await;
}

#[tokio::test]
async fn an_invalid_settings_patch_is_a_bad_request() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            let response = raw(
                addr,
                &body("PATCH", SETTINGS, Some(TOKEN), r#"{"cutoff_celsius":26.5}"#),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn an_on_override_is_accepted_with_a_duration() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            let status: HeaterStatus = parse(
                &raw(
                    addr,
                    &body(
                        "PUT",
                        TANK_OVERRIDE,
                        Some(TOKEN),
                        r#"{"state":"on","duration_s":600}"#,
                    ),
                )
                .await,
            );

            assert_eq!(status.mode, HeaterMode::ManualOn);
            let expires = status.override_expires_in_s.expect("an expiry");
            assert!((599..=600).contains(&expires), "got {expires}");
        })
        .await;
}

#[tokio::test]
async fn an_override_with_an_invalid_duration_is_refused() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            for payload in [
                r#"{"state":"on"}"#,
                r#"{"state":"on","duration_s":0}"#,
                r#"{"state":"on","duration_s":3601}"#,
                r#"{"state":"off","duration_s":0}"#,
            ] {
                let response = raw(addr, &body("PUT", TANK_OVERRIDE, Some(TOKEN), payload)).await;
                assert!(
                    response.starts_with("HTTP/1.1 400"),
                    "{payload} got: {response}"
                );
            }
        })
        .await;
}

#[tokio::test]
async fn an_unknown_heater_is_not_found() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            let response = raw(
                addr,
                &body(
                    "PUT",
                    "/api/heaters/boiler/override",
                    Some(TOKEN),
                    r#"{"state":"off"}"#,
                ),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");

            let response = raw(
                addr,
                &body("DELETE", "/api/heaters/boiler/override", Some(TOKEN), ""),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 404"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn deleting_the_override_returns_to_auto() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;
            let set: HeaterStatus = parse(
                &raw(
                    addr,
                    &body("PUT", TANK_OVERRIDE, Some(TOKEN), r#"{"state":"off"}"#),
                )
                .await,
            );
            assert_eq!(set.mode, HeaterMode::ManualOff);
            assert_eq!(set.override_expires_in_s, None);

            let response = raw(addr, &body("DELETE", TANK_OVERRIDE, Some(TOKEN), "")).await;
            assert!(response.starts_with("HTTP/1.1 204"), "got: {response}");

            let status: HeatersStatus = parse(&raw(addr, &get(STATUS, Some(TOKEN))).await);
            assert_eq!(status.tank.mode, HeaterMode::Auto);
        })
        .await;
}

#[tokio::test]
async fn a_sump_override_leaves_the_tank_in_auto() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_heaters(heaters()).await;

            let response = raw(
                addr,
                &body(
                    "PUT",
                    "/api/heaters/sump/override",
                    Some(TOKEN),
                    r#"{"state":"off"}"#,
                ),
            )
            .await;
            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");

            let status: HeatersStatus = parse(&raw(addr, &get(STATUS, Some(TOKEN))).await);
            assert_eq!(status.sump.mode, HeaterMode::ManualOff);
            assert_eq!(status.tank.mode, HeaterMode::Auto);
        })
        .await;
}
