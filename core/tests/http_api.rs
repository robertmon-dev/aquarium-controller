use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::LocalSet;

use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{InMemoryLightingStore, InMemoryTelemetryStore};
use aquarium_core::domain::metrics::TankTelemetryDto;
use aquarium_core::ports::outbound::TelemetrySink;

const TOKEN: &str = "test-token";

fn config() -> HttpConfig {
    HttpConfig {
        token: TOKEN,
        max_connections: 4,
        sse_keepalive: Duration::from_secs(15),
    }
}

async fn start(store: Arc<InMemoryTelemetryStore>) -> SocketAddr {
    start_with_lighting(store, Arc::new(InMemoryLightingStore::new())).await
}

async fn start_with_lighting(
    store: Arc<InMemoryTelemetryStore>,
    lighting: Arc<InMemoryLightingStore>,
) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(listener, store, lighting, config()));

    addr
}

fn body(method: &str, path: &str, auth: Option<&str>, json: &str) -> String {
    let auth_line = match auth {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };

    format!(
        "{method} {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}\
         Content-Type: application/json\r\nContent-Length: {}\r\n\
         Connection: close\r\n\r\n{json}",
        json.len()
    )
}

async fn raw(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");

    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(500), stream.read_to_end(&mut buf)).await;

    String::from_utf8_lossy(&buf).into_owned()
}

fn get(path: &str, auth: Option<&str>) -> String {
    let auth_line = match auth {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };

    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}Connection: close\r\n\r\n")
}

#[tokio::test]
async fn telemetry_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/telemetry", None)).await;
            assert!(response.starts_with("HTTP/1.1 401"), "dostałem: {response}");
        })
        .await;
}

#[tokio::test]
async fn telemetry_with_a_wrong_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/telemetry", Some("wrong"))).await;
            assert!(response.starts_with("HTTP/1.1 401"), "dostałem: {response}");
        })
        .await;
}

#[tokio::test]
async fn telemetry_with_a_valid_token_returns_the_snapshot() {
    LocalSet::new()
        .run_until(async {
            let store = Arc::new(InMemoryTelemetryStore::new());
            let mut snapshot = TankTelemetryDto::empty();
            snapshot.temperature.tank_celsius = Some(26.5);
            snapshot.system_health.uptime_seconds = 99;
            store.publish(snapshot);

            let addr = start(Arc::clone(&store)).await;
            let response = raw(addr, &get("/api/telemetry", Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "dostałem: {response}");
            assert!(
                response.contains("application/json"),
                "dostałem: {response}"
            );
            assert!(
                response.contains("26.5"),
                "brak odczytu w ciele: {response}"
            );
            assert!(
                response.contains("\"uptime_seconds\":99"),
                "dostałem: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn query_token_is_rejected_outside_the_events_route() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &format!(
                    "GET /api/telemetry?token={TOKEN} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
                ),
            )
            .await;

            assert!(
                response.starts_with("HTTP/1.1 401"),
                "furtka na query musi obowiązywać WYŁĄCZNIE na /api/events, dostałem: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn events_accepts_a_query_token_and_streams() {
    LocalSet::new()
        .run_until(async {
            let store = Arc::new(InMemoryTelemetryStore::new());
            let addr = start(Arc::clone(&store)).await;

            let mut stream = TcpStream::connect(addr).await.expect("connect");
            stream
                .write_all(
                    format!("GET /api/events?token={TOKEN} HTTP/1.1\r\nHost: localhost\r\n\r\n")
                        .as_bytes(),
                )
                .await
                .expect("write");

            let mut buf = [0_u8; 2048];
            let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
                .await
                .expect("SSE nie odpowiedziało w 2 s")
                .expect("read");

            let response = String::from_utf8_lossy(&buf[..read]).into_owned();
            assert!(response.starts_with("HTTP/1.1 200"), "dostałem: {response}");
            assert!(
                response.contains("text/event-stream"),
                "dostałem: {response}"
            );
            assert!(
                response.contains("event:telemetry"),
                "nagłówki to nie dowód — brak faktycznego zdarzenia: {response}"
            );
        })
        .await;
}

#[tokio::test]
async fn events_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/events", None)).await;
            assert!(response.starts_with("HTTP/1.1 401"), "dostałem: {response}");
        })
        .await;
}

#[tokio::test]
async fn health_with_a_valid_token_returns_system_health() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/health", Some(TOKEN))).await;

            assert!(response.starts_with("HTTP/1.1 200"), "dostałem: {response}");
            assert!(response.contains("free_heap_kb"), "dostałem: {response}");
        })
        .await;
}

#[tokio::test]
async fn unknown_path_with_a_valid_token_is_not_found() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(addr, &get("/api/nope", Some(TOKEN))).await;
            assert!(response.starts_with("HTTP/1.1 404"), "dostałem: {response}");
        })
        .await;
}

const OVERRIDE_PATH: &str = "/api/lighting/override";

#[tokio::test]
async fn setting_an_override_without_a_token_is_rejected() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    OVERRIDE_PATH,
                    None,
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;
            assert!(response.starts_with("HTTP/1.1 401"), "got: {response}");
        })
        .await;
}

#[tokio::test]
async fn a_query_token_does_not_open_the_override_route() {
    LocalSet::new()
        .run_until(async {
            let addr = start(Arc::new(InMemoryTelemetryStore::new())).await;
            let response = raw(
                addr,
                &body(
                    "PUT",
                    &format!("{OVERRIDE_PATH}?token={TOKEN}"),
                    None,
                    r#"{"target":{"profile":"off"}}"#,
                ),
            )
            .await;
            assert!(
                response.starts_with("HTTP/1.1 401"),
                "the query-token fallback must apply ONLY to /api/events, got: {response}"
            );
        })
        .await;
}

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
