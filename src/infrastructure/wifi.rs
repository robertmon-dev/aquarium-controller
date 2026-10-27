use std::time::Duration;

use esp_idf_svc::eventloop::EspSystemEventLoop;
use esp_idf_svc::hal::modem::WifiModemPeripheral;
use esp_idf_svc::nvs::EspDefaultNvsPartition;
use esp_idf_svc::sys::EspError;
use esp_idf_svc::timer::EspTaskTimerService;
use esp_idf_svc::wifi::{AsyncWifi, ClientConfiguration, Configuration, EspWifi, PowerSaveMode};
use tokio::time::sleep;
use tracing::{info, warn};

use crate::settings::WifiSettings;

const RETRY_MIN: Duration = Duration::from_secs(2);
const RETRY_MAX: Duration = Duration::from_secs(60);

#[derive(Debug, thiserror::Error)]
pub enum WifiError {
    #[error("WiFi driver error: {0}")]
    Driver(#[from] EspError),

    #[error("SSID is too long (max 32 bytes)")]
    SsidTooLong,

    #[error("Password is too long (max 64 bytes)")]
    PasswordTooLong,
}

pub struct WifiManager {
    wifi: AsyncWifi<EspWifi<'static>>,
}

impl WifiManager {
    pub fn new<M: WifiModemPeripheral + 'static>(
        modem: M,
        sys_loop: EspSystemEventLoop,
        nvs: EspDefaultNvsPartition,
        config: &WifiSettings,
    ) -> Result<Self, WifiError> {
        let timer_service = EspTaskTimerService::new()?;

        let mut wifi = AsyncWifi::wrap(
            EspWifi::new(modem, sys_loop.clone(), Some(nvs))?,
            sys_loop,
            timer_service,
        )?;

        wifi.set_configuration(&Configuration::Client(ClientConfiguration {
            ssid: config.ssid.try_into().map_err(|_| WifiError::SsidTooLong)?,
            password: config
                .password
                .expose()
                .try_into()
                .map_err(|_| WifiError::PasswordTooLong)?,
            auth_method: config.auth_method,
            ..Default::default()
        }))?;

        wifi.wifi_mut().set_power_save(PowerSaveMode::None)?;

        Ok(Self { wifi })
    }

    pub async fn run(mut self) {
        let mut backoff = RETRY_MIN;

        loop {
            match self.connect().await {
                Ok(()) => {
                    backoff = RETRY_MIN;
                    self.log_ip();

                    if let Err(e) = self.wifi.wifi_wait(|w| w.is_connected(), None).await {
                        warn!(error = %e, "Failed while waiting for WiFi disconnect");
                        sleep(RETRY_MIN).await;
                    }

                    warn!("WiFi disconnected, reconnecting");
                }
                Err(e) => {
                    warn!(error = %e, retry_in_s = backoff.as_secs(), "WiFi connection failed");

                    let _ = self.wifi.disconnect().await;

                    sleep(backoff).await;
                    backoff = (backoff * 2).min(RETRY_MAX);
                }
            }
        }
    }

    async fn connect(&mut self) -> Result<(), EspError> {
        if !self.wifi.is_started()? {
            self.wifi.start().await?;
        }
        self.wifi.connect().await?;
        self.wifi.wait_netif_up().await?;
        Ok(())
    }

    fn log_ip(&self) {
        match self.wifi.wifi().sta_netif().get_ip_info() {
            Ok(ip) => info!(ip = %ip.ip, "WiFi connected"),
            Err(e) => info!(error = %e, "WiFi connected (IP info unavailable)"),
        }
    }
}
