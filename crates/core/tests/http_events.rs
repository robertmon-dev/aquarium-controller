mod common;

use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::LocalSet;

use aquarium_core::adapters::outbound::memory::InMemoryTelemetryStore;

use common::{start, TOKEN};

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
                .expect("SSE did not answer within 2 s")
                .expect("read");

            let response = String::from_utf8_lossy(&buf[..read]).into_owned();
            assert!(response.starts_with("HTTP/1.1 200"), "got: {response}");
            assert!(response.contains("text/event-stream"), "got: {response}");
            assert!(
                response.contains("event:telemetry"),
                "headers are no proof - no actual event arrived: {response}"
            );
        })
        .await;
}
