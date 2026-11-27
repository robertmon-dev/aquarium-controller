use std::future::Future;
use std::sync::Arc;

use anyhow::Context;
use aquarium_core::application::{
    lighting_loop, sump_light_loop, LightingService, SumpLightService,
};
use aquarium_core::domain::lighting::LightingPalette;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use tokio::sync::Notify;
use tracing::info;

use aquarium_controller::adapters::outbound::lighting::RgbwOutputs;
use aquarium_controller::adapters::outbound::nvs_lighting::NvsLightingPalette;
use aquarium_controller::adapters::outbound::sump_light::SumpLightOutput;
use aquarium_controller::infrastructure::clock::SystemClock;

use crate::board::{RgbwPins, SumpLightPin};
use crate::SETTINGS;

pub(super) struct Lighting {
    pub service: Arc<LightingService<NvsLightingPalette>>,
    pub republish: Arc<Notify>,
}

impl Lighting {
    pub fn load(nvs_partition: Option<EspDefaultNvsPartition>) -> anyhow::Result<Self> {
        let persistence = NvsLightingPalette::new(nvs_partition)
            .context("Failed to open the lighting namespace in NVS")?;

        Ok(Self {
            service: Arc::new(LightingService::new(LightingPalette::DEFAULT, persistence)),
            republish: Arc::new(Notify::new()),
        })
    }

    pub fn task(
        &self,
        pins: RgbwPins,
        clock: Arc<SystemClock>,
    ) -> anyhow::Result<(
        Option<impl Future<Output = ()>>,
        Option<RgbwOutputs<'static>>,
    )> {
        let component = &SETTINGS.components.lighting;
        let outputs = component
            .is_initialized()
            .then(|| pins.open(SETTINGS.lighting.pwm_freq_hz))
            .transpose()?;

        Ok(match (outputs, component.is_running()) {
            (Some(outputs), true) => {
                info!("Lighting control running");
                (
                    Some(lighting_loop(
                        clock,
                        Arc::clone(&self.service),
                        outputs,
                        Arc::clone(&self.republish),
                        SETTINGS.lighting.update_interval,
                    )),
                    None,
                )
            }
            (Some(outputs), false) => {
                info!("RGBW outputs held at 0 % but not controlled");
                (None, Some(outputs))
            }
            (None, _) => (None, None),
        })
    }
}

pub(super) struct SumpLight {
    pub service: Arc<SumpLightService>,
    pub republish: Arc<Notify>,
}

impl SumpLight {
    pub fn new() -> Self {
        Self {
            service: Arc::new(SumpLightService::new()),
            republish: Arc::new(Notify::new()),
        }
    }

    pub fn task(
        &self,
        pin: SumpLightPin,
    ) -> anyhow::Result<(
        Option<impl Future<Output = ()>>,
        Option<SumpLightOutput<'static>>,
    )> {
        let component = &SETTINGS.components.sump_light;
        let output = component.is_initialized().then(|| pin.open()).transpose()?;

        Ok(match (output, component.is_running()) {
            (Some(output), true) => {
                info!("Sump light control running");
                (
                    Some(sump_light_loop(
                        Arc::clone(&self.service),
                        output,
                        Arc::clone(&self.republish),
                    )),
                    None,
                )
            }
            (Some(output), false) => {
                info!("Sump light output held low but not controlled");
                (None, Some(output))
            }
            (None, _) => (None, None),
        })
    }
}
