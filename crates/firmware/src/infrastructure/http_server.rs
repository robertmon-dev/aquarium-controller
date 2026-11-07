use std::net::TcpListener as StdTcpListener;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use aquarium_core::adapters::inbound::http::{self, HttpConfig};
use aquarium_core::ports::inbound::{LightingControl, TelemetryQuery};
use esp_idf_hal::task::thread::{MallocCap, ThreadSpawnConfiguration};
use esp_idf_svc::sys;
use tokio::net::TcpListener;
use tokio::task::LocalSet;
use tokio::time::MissedTickBehavior;
use tracing::{error, info};

const LOW_HEADROOM_BYTES: u32 = 16384;

pub fn spawn<Q, L>(
    listener: StdTcpListener,
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    config: HttpConfig,
    stack_size: usize,
    health_interval: Option<Duration>,
) -> anyhow::Result<()>
where
    Q: TelemetryQuery + Send + Sync + 'static,
    L: LightingControl + Send + Sync + 'static,
{
    listener
        .set_nonblocking(true)
        .context("Failed to set the HTTP listener non-blocking")?;

    ThreadSpawnConfiguration {
        name: Some(c"http"),
        stack_size,
        stack_alloc_caps: MallocCap::Spiram | MallocCap::Cap8bit,
        ..Default::default()
    }
    .set()
    .context("Failed to configure the HTTP thread")?;

    let spawned = std::thread::Builder::new()
        .spawn(move || serve(listener, telemetry, lighting, config, health_interval));

    ThreadSpawnConfiguration::default()
        .set()
        .context("Failed to restore the default thread configuration")?;

    spawned.context("Failed to spawn the HTTP thread")?;

    info!(stack_size, "HTTP thread spawned with a PSRAM stack");

    Ok(())
}

fn serve<Q, L>(
    listener: StdTcpListener,
    telemetry: Arc<Q>,
    lighting: Arc<L>,
    config: HttpConfig,
    health_interval: Option<Duration>,
) where
    Q: TelemetryQuery + 'static,
    L: LightingControl + 'static,
{
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            error!(%error, "HTTP thread could not build its runtime");
            return;
        }
    };

    let local = LocalSet::new();

    runtime.block_on(local.run_until(async move {
        let listener = match TcpListener::from_std(listener) {
            Ok(listener) => listener,
            Err(error) => {
                error!(%error, "HTTP thread could not adopt the listener");
                return;
            }
        };

        if let Some(interval) = health_interval {
            tokio::task::spawn_local(report_health(interval));
        }

        http::run(listener, telemetry, lighting, config).await
    }));
}

async fn report_health(interval: Duration) {
    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        let stack_free = unsafe { sys::uxTaskGetStackHighWaterMark(core::ptr::null_mut()) };

        if stack_free < LOW_HEADROOM_BYTES {
            error!(
                task = "http",
                stack_free, "http thread stack headroom is low"
            );
        }

        info!(task = "http", stack_free, "health");
    }
}
