use std::time::Duration;

use anyhow::Context;
use esp_idf_hal::peripherals::Peripherals;
use esp_idf_svc::io::vfs::MountedEventfs;
use esp_idf_svc::sys::link_patches;
use tokio::time::MissedTickBehavior;
use tracing::{info, warn};

use aquarium_controller::adapters::outbound::ds18b20::{Ds18b20Adapter, OneWireBus, RomAddress};
use aquarium_controller::ports::outbound::temperature::TemperatureReader;

/// Those are placeholders `Ds18b20Adapter::new` gonna throw with error
const TANK_ROM: RomAddress = [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
const SUMP_ROM: RomAddress = [0x28, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];

const ROM_DISCOVERY_MODE: bool = false;
const READ_INTERVAL: Duration = Duration::from_secs(30);

fn main() -> anyhow::Result<()> {
    link_patches();
    init_logging();

    info!("Booted system controller");

    let _eventfs = MountedEventfs::mount(5).context("Failed to mount eventfd VFS for tokio")?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build tokio runtime")?;

    runtime.block_on(run())
}

fn init_logging() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_target(false)
        .without_time()
        .compact()
        .init();
}

async fn run() -> anyhow::Result<()> {
    let peripherals = Peripherals::take().context("Peripherals already taken")?;

    let mut bus =
        OneWireBus::new(peripherals.pins.gpio4).context("Failed to initialize 1-Wire bus")?;

    if ROM_DISCOVERY_MODE {
        return discover_rom(&mut bus);
    }

    let mut sensors = Ds18b20Adapter::new(bus, TANK_ROM, SUMP_ROM)
        .context("Failed to initialize DS18B20 sensors")?;

    info!("Temperature sensors ready");

    let mut ticker = tokio::time::interval(READ_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        match sensors.read_all().await {
            Ok(readings) => {
                for (role, temp) in readings {
                    info!(?role, celsius = temp.as_celsius(), "Water temperature");
                }
            }
            Err(e) => warn!(error = %e, "Temperature read failed"),
        }
    }
}

fn discover_rom(bus: &mut OneWireBus<'_>) -> anyhow::Result<()> {
    let rom = bus
        .read_single_rom()
        .context("ROM discovery failed; connect exactly ONE sensor")?;

    let formatted = rom
        .iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ");

    info!("Sensor ROM: [{formatted}]");
    Ok(())
}
