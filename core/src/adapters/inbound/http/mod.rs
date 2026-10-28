pub mod auth;
pub mod handlers;
pub mod routes;
pub mod sse;

use std::sync::Arc;
use std::time::Duration;

use picoserve::time::Duration as PicoDuration;
use tokio::net::TcpListener;
use tokio::sync::Semaphore;
use tracing::{info, warn};

use crate::ports::inbound::{LightingControl, TelemetryQuery};

const HTTP_BUFFER_BYTES: usize = 2048;

pub struct HttpConfig {
    pub token: &'static str,
    pub max_connections: usize,
    pub sse_keepalive: Duration,
}

pub async fn run<Q, L>(
    listener: TcpListener,
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    config: HttpConfig,
) -> !
where
    Q: TelemetryQuery + 'static,
    L: LightingControl + 'static,
{
    let router: &'static _ = Box::leak(Box::new(routes::router(
        telemetry,
        lighting,
        config.token,
        config.sse_keepalive,
    )));

    let server_config: &'static _ = Box::leak(Box::new(
        picoserve::Config::new(picoserve::Timeouts {
            start_read_request: PicoDuration::from_secs(5),
            persistent_start_read_request: PicoDuration::from_secs(1),
            read_request: PicoDuration::from_secs(3),
            write: PicoDuration::from_secs(5),
        })
        .keep_connection_alive(),
    ));

    let slots = Arc::new(Semaphore::new(config.max_connections));

    info!(slots = config.max_connections, "HTTP server listening");

    loop {
        let (stream, peer) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(error) => {
                warn!(%error, "accept failed");
                continue;
            }
        };

        let permit = match Arc::clone(&slots).try_acquire_owned() {
            Ok(permit) => permit,
            Err(_) => {
                warn!(%peer, "connection rejected: all slots busy");
                drop(stream);
                continue;
            }
        };

        let accepted_at = std::time::Instant::now();

        tokio::task::spawn_local(async move {
            let mut buffer = vec![0_u8; HTTP_BUFFER_BYTES];

            let result = picoserve::Server::new_tokio(router, server_config, &mut buffer)
                .serve(stream)
                .await;

            let total_ms = accepted_at.elapsed().as_millis();

            match result {
                Ok(_) => info!(%peer, total_ms, "connection closed"),
                Err(error) => info!(%peer, total_ms, ?error, "connection ended"),
            }

            drop(permit);
        });
    }
}
