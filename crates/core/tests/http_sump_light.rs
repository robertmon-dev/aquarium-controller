#![recursion_limit = "256"]

mod common;

use serde_json::Value;
use tokio::task::LocalSet;

use common::{body, get, raw, start_with_sump_light, sump_light, TOKEN};

const PATH: &str = "/api/sump-light";

fn json(response: &str) -> Value {
    let (_, payload) = response.split_once("\r\n\r\n").expect("headers end");
    serde_json::from_str(payload).expect("json body")
}

#[tokio::test]
async fn status_is_unavailable_without_the_control_loop() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_sump_light(sump_light()).await;
            let response = raw(addr, &get(PATH, Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            assert_eq!(json(&response)["available"], false);
        })
        .await;
}

#[tokio::test]
async fn a_timed_on_request_is_accepted() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_sump_light(sump_light()).await;
            let response = raw(
                addr,
                &body("PUT", PATH, Some(TOKEN), r#"{"on":true,"duration_s":1800}"#),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            let status = json(&response);
            assert_eq!(status["available"], false);
            assert_eq!(status["expires_in_s"], Value::Null);
        })
        .await;
}

#[tokio::test]
async fn off_with_a_duration_is_a_bad_request() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_sump_light(sump_light()).await;
            let response = raw(
                addr,
                &body("PUT", PATH, Some(TOKEN), r#"{"on":false,"duration_s":60}"#),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn a_too_short_duration_is_a_bad_request() {
    LocalSet::new()
        .run_until(async {
            let addr = start_with_sump_light(sump_light()).await;
            let response = raw(
                addr,
                &body("PUT", PATH, Some(TOKEN), r#"{"on":true,"duration_s":10}"#),
            )
            .await;

            assert!(response.starts_with("HTTP/1.1 400"), "got: {response}");
        })
        .await;
}
