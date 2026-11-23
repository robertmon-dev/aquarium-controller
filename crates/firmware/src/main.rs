mod board;
mod diagnostics;
mod startup;

use anyhow::Context;
use aquarium_controller::settings::{RunMode, Settings};
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::io::vfs::MountedEventfs;
use esp_idf_svc::sys::link_patches;
use tokio::task::LocalSet;
use tracing::info;

const SETTINGS: Settings = Settings::from_env();

fn main() -> anyhow::Result<()> {
    link_patches();
    init_logging();

    info!("Booted system controller");

    let _eventfs = MountedEventfs::mount(5).context("Failed to mount eventfd VFS for tokio")?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    let local = LocalSet::new();
    runtime.block_on(local.run_until(Box::pin(run())))
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .compact()
        .init();
}

async fn run() -> anyhow::Result<()> {
    let board = board::Board::from(Peripherals::take().context("Peripherals already taken")?);
    match SETTINGS.run_mode {
        RunMode::Normal => startup::run(board).await,
        RunMode::RomDiscovery => diagnostics::discover_roms(&mut board.temperature.open()?).await,
        RunMode::AdcProbe => diagnostics::adc_probe(board.analog).await,
        RunMode::SsrProbe => diagnostics::ssr_probe(&mut board.heaters.open()?).await,
        RunMode::RelayProbe => {
            diagnostics::probe_relays(&mut board.pumps.open()?, &mut board.reservoir.open()?).await
        }
    }
}
