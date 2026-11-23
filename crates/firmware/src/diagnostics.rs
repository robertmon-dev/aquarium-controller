use anyhow::Context;
use aquarium_controller::adapters::outbound::ds18b20::OneWireBus;
use aquarium_controller::adapters::outbound::relay::{RelayOutputs, ReservoirRelayOutputs};
use aquarium_controller::adapters::outbound::ssr::SsrOutputs;
use aquarium_core::application::heater::probe_heaters;
use aquarium_core::domain::pump::PumpId;
use aquarium_core::domain::reservoir::ReservoirChannel;
use aquarium_core::protocol::ads1115::Channel;
use aquarium_core::protocol::ds18b20::{
    convert_all, read_scratchpad, search_roms, temperature_from_scratchpad, RomAddress,
};
use std::time::Duration;
use tokio::time::{sleep, MissedTickBehavior};
use tracing::{error, info};

use crate::board::AnalogBus;

const PROBE_INTERVAL: Duration = Duration::from_secs(2);

const DISCOVERY_SENSOR_LIMIT: usize = 8;
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(2);
const DISCOVERY_CONVERSION_WAIT: Duration = Duration::from_millis(800);

pub async fn ssr_probe(outputs: &mut SsrOutputs<'_>) -> anyhow::Result<()> {
    info!("SSR probe: one channel at a time, GPIO17 tank SSR, then GPIO18 sump SSR");
    probe_heaters(outputs, PROBE_INTERVAL).await
}

pub async fn adc_probe(bus: AnalogBus) -> anyhow::Result<()> {
    let mut front_end = bus.open(Some(Channel::A1), Some(Channel::A0))?;
    let mut ticker = tokio::time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    info!("ADC probe: A0 is the TDS probe, A1 is the pH probe");

    loop {
        ticker.tick().await;

        for (label, channel) in [("A0 tds", Channel::A0), ("A1 ph", Channel::A1)] {
            match front_end.probe(channel).await {
                Ok((raw, volts)) => info!(raw, volts, "{label}"),
                Err(reason) => error!(reason, "{label}"),
            }
        }
    }
}

const BARREL_PROBE: [(ReservoirChannel, bool); 8] = [
    (ReservoirChannel::RoValve, true),
    (ReservoirChannel::Booster, true),
    (ReservoirChannel::Booster, false),
    (ReservoirChannel::RoValve, false),
    (ReservoirChannel::TapValve, true),
    (ReservoirChannel::TapValve, false),
    (ReservoirChannel::Mixer, true),
    (ReservoirChannel::Mixer, false),
];

pub async fn probe_relays(
    relays: &mut RelayOutputs<'_>,
    barrel: &mut ReservoirRelayOutputs<'_>,
) -> anyhow::Result<()> {
    let mut ticker = tokio::time::interval(PROBE_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    info!(
        "Relay probe: module 1 IN1 GPIO5 return, IN2 GPIO6 uv, IN3 GPIO7 makeup, IN4 GPIO16 drain"
    );
    info!("Relay probe: module 2 IN1 GPIO40 ro_valve, IN2 GPIO41 booster, IN3 GPIO42 tap_valve, IN4 GPIO47 mixer");

    loop {
        for id in PumpId::ALL {
            for coil in [true, false] {
                ticker.tick().await;
                match relays.set_coil(id, coil) {
                    Ok(()) => info!(channel = id.name(), coil, "relay"),
                    Err(reason) => error!(channel = id.name(), reason, "relay"),
                }
            }
        }

        for (channel, coil) in BARREL_PROBE {
            ticker.tick().await;
            match barrel.set_coil(channel, coil) {
                Ok(()) => info!(channel = channel.name(), coil, "barrel relay"),
                Err(reason) => {
                    for output in [
                        ReservoirChannel::Booster,
                        ReservoirChannel::RoValve,
                        ReservoirChannel::TapValve,
                        ReservoirChannel::Mixer,
                    ] {
                        let _ = barrel.set_coil(output, false);
                    }
                    anyhow::bail!("barrel relay probe stopped at {}: {reason}", channel.name());
                }
            }
        }
    }
}

fn format_rom(rom: &RomAddress) -> String {
    rom.iter()
        .map(|b| format!("0x{b:02X}"))
        .collect::<Vec<_>>()
        .join(", ")
}

pub async fn discover_roms(bus: &mut OneWireBus<'_>) -> anyhow::Result<()> {
    let mut found = [[0u8; 8]; DISCOVERY_SENSOR_LIMIT];
    let count = search_roms(bus, &mut found).context("SEARCH ROM failed")?;

    if count == 0 {
        anyhow::bail!("No DS18B20 responded on the 1-Wire bus");
    }

    for rom in &found[..count] {
        info!("Sensor ROM: [{}]", format_rom(rom));
    }

    info!(
        count,
        "Hold one probe in your hand; the ROM whose reading climbs is that sensor"
    );

    let mut ticker = tokio::time::interval(DISCOVERY_INTERVAL);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;

        convert_all(bus).context("Failed to start a temperature conversion")?;
        sleep(DISCOVERY_CONVERSION_WAIT).await;

        for rom in &found[..count] {
            match read_scratchpad(bus, rom).and_then(|sp| temperature_from_scratchpad(&sp)) {
                Ok(temperature) => {
                    info!(celsius = temperature.as_celsius(), "[{}]", format_rom(rom))
                }
                Err(error) => error!(%error, "[{}]", format_rom(rom)),
            }
        }
    }
}
