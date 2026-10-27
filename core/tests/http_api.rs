use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::LocalSet;

use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;
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
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(listener, store, config()));

    addr
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
