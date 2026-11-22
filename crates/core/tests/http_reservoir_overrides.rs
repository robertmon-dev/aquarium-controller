#![recursion_limit = "256"]

mod common;

use std::sync::Arc;
use std::time::Duration;

use tokio::task::LocalSet;
use tokio::time::Instant;

use aquarium_core::application::reservoir::Observation;
use aquarium_core::domain::reservoir::{ChannelMode, ReservoirConditions, ReservoirStatus};
use aquarium_core::ports::inbound::ReservoirControl;

use common::{
    assert_refused, body, delete_at, observe_barrel, parse_body, post_fill, raw, ready_barrel,
    reservoir, start_with_reservoir, status_of, HALF_BARREL, TOKEN,
};

#[tokio::test]
async fn a_channel_runtime_is_reset() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_reservoir(reservoir()).await;
            let reset = |channel: &str| {
                body(
                    "POST",
                    &format!("/api/reservoir/{channel}/runtime/reset"),
                    Some(TOKEN),
                    "",
                )
            };

            for channel in ["ro_valve", "booster", "tap_valve", "mixer"] {
                assert_eq!(status_of(&raw(addr, &reset(channel)).await), 204);
            }
            assert_eq!(status_of(&raw(addr, &reset("ro")).await), 404);
        })
        .await;
}

#[tokio::test]
async fn overrides_are_validated_and_refused_with_reasons() {
    LocalSet::new()
        .run_until(async {
            let reservoir = ready_barrel();
            let addr = start_with_reservoir(Arc::clone(&reservoir)).await;
            let put = |target: &str, json: &str| {
                body(
                    "PUT",
                    &format!("/api/reservoir/{target}/override"),
                    Some(TOKEN),
                    json,
                )
            };
            let on = r#"{"on":true,"duration_s":60}"#;

            assert_eq!(status_of(&raw(addr, &put("booster", on)).await), 404);
            assert_refused(
                &raw(addr, &put("ro", r#"{"on":true}"#)).await,
                400,
                "an ON override needs duration_s",
            );
            assert_refused(
                &raw(addr, &put("ro", r#"{"on":true,"duration_s":0}"#)).await,
                400,
                "an override lasts 1 to 600 s",
            );
            assert_refused(
                &raw(addr, &put("ro", r#"{"on":true,"duration_s":601}"#)).await,
                400,
                "an override lasts 1 to 600 s",
            );

            let accepted = raw(addr, &put("ro", on)).await;
            assert_eq!(status_of(&accepted), 200, "got: {accepted}");
            let status: ReservoirStatus = parse_body(&accepted);
            assert_eq!(status.channels.ro_valve.mode, ChannelMode::ManualOn);
            assert_eq!(status.channels.ro_valve.override_expires_in_s, Some(60));

            assert_refused(
                &raw(addr, &put("tap", on)).await,
                423,
                "the other inlet valve may still be open",
            );
            assert_eq!(
                status_of(&delete_at(addr, "/api/reservoir/ro/override").await),
                204
            );
            assert_eq!(reservoir.status().channels.ro_valve.mode, ChannelMode::Auto);

            observe_barrel(&reservoir, Some(HALF_BARREL), true);
            assert_refused(
                &raw(addr, &put("tap", on)).await,
                423,
                "a water change is running; the barrel fills after it",
            );
            observe_barrel(
                &reservoir,
                Some(ReservoirConditions {
                    full: true,
                    ..HALF_BARREL
                }),
                false,
            );
            assert_refused(&raw(addr, &put("ro", on)).await, 423, "the barrel is full");
            let empty = ReservoirConditions {
                empty: true,
                ready: false,
                ..HALF_BARREL
            };
            observe_barrel(&reservoir, Some(empty), false);
            assert_refused(
                &raw(addr, &put("mixer", on)).await,
                423,
                "the barrel is empty, the mixer would run dry",
            );
            let off = raw(addr, &put("mixer", r#"{"on":false}"#)).await;
            assert_eq!(status_of(&off), 200, "got: {off}");
            let status: ReservoirStatus = parse_body(&off);
            assert_eq!(status.channels.mixer.mode, ChannelMode::ManualOff);

            observe_barrel(&reservoir, Some(HALF_BARREL), false);
            assert_eq!(status_of(&post_fill(addr).await), 202);
            assert_refused(
                &raw(addr, &put("mixer", on)).await,
                409,
                "a barrel fill is running; cancel it before overriding",
            );
        })
        .await;
}

#[tokio::test]
async fn an_inlet_override_is_refused_while_the_inlet_rests() {
    LocalSet::new()
        .run_until(async {
            let reservoir = ready_barrel();
            let max_on = reservoir.settings().inlet_max_on_s;
            reservoir.observe(Observation {
                ran: [true, false, false, false],
                elapsed: Duration::from_secs(u64::from(max_on) + 1),
                ..Observation::idle(Some(HALF_BARREL), Instant::now())
            });
            let addr = start_with_reservoir(reservoir).await;
            let put = |target: &str| {
                body(
                    "PUT",
                    &format!("/api/reservoir/{target}/override"),
                    Some(TOKEN),
                    r#"{"on":true,"duration_s":60}"#,
                )
            };

            assert_refused(
                &raw(addr, &put("ro")).await,
                423,
                "the inlet is resting after a long run",
            );
            assert_eq!(status_of(&raw(addr, &put("tap")).await), 200);
        })
        .await;
}

#[tokio::test]
async fn an_on_override_is_refused_by_a_global_blocker() {
    LocalSet::new()
        .run_until(async {
            let reservoir = reservoir();
            observe_barrel(&reservoir, Some(HALF_BARREL), false);
            let addr = start_with_reservoir(Arc::clone(&reservoir)).await;
            let put = |json: &str| body("PUT", "/api/reservoir/mixer/override", Some(TOKEN), json);

            assert_refused(
                &raw(addr, &put(r#"{"on":true,"duration_s":60}"#)).await,
                423,
                "the controller watchdog is not supervising the barrel",
            );
            assert_eq!(status_of(&raw(addr, &put(r#"{"on":false}"#)).await), 200);
        })
        .await;
}
