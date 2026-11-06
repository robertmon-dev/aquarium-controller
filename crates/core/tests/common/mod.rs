#![allow(dead_code)]

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::adapters::outbound::memory::{InMemoryLightingStore, InMemoryTelemetryStore};

pub const TOKEN: &str = "test-token";
pub const ORIGIN: &str = "http://localhost:8081";
pub const OVERRIDE_PATH: &str = "/api/lighting/override";

pub fn config() -> HttpConfig {
    HttpConfig {
        token: TOKEN,
        max_connections: 4,
        sse_keepalive: Duration::from_secs(15),
        cors_origin: ORIGIN,
    }
}

pub async fn start(store: Arc<InMemoryTelemetryStore>) -> SocketAddr {
    start_with_lighting(store, Arc::new(InMemoryLightingStore::new())).await
}

pub async fn start_with_lighting(
    store: Arc<InMemoryTelemetryStore>,
    lighting: Arc<InMemoryLightingStore>,
) -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind");
    let addr = listener.local_addr().expect("local_addr");

    tokio::task::spawn_local(http::run(listener, store, lighting, config()));

    addr
}

pub fn body(method: &str, path: &str, auth: Option<&str>, json: &str) -> String {
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

pub fn get(path: &str, auth: Option<&str>) -> String {
    let auth_line = match auth {
        Some(token) => format!("Authorization: Bearer {token}\r\n"),
        None => String::new(),
    };

    format!("GET {path} HTTP/1.1\r\nHost: localhost\r\n{auth_line}Connection: close\r\n\r\n")
}

pub async fn raw(addr: SocketAddr, request: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream.write_all(request.as_bytes()).await.expect("write");

    let mut buf = Vec::new();
    let _ = tokio::time::timeout(Duration::from_millis(500), stream.read_to_end(&mut buf)).await;

    String::from_utf8_lossy(&buf).into_owned()
}

pub fn preflight(path: &str, method: &str) -> String {
    format!(
        "OPTIONS {path} HTTP/1.1\r\nHost: localhost\r\nOrigin: {ORIGIN}\r\n\
         Access-Control-Request-Method: {method}\r\n\
         Access-Control-Request-Headers: authorization\r\n\
         Connection: close\r\n\r\n"
    )
}
