use std::time::{SystemTime, UNIX_EPOCH};

use aquarium_core::ports::outbound::Clock;
use esp_idf_svc::sntp::{EspSntp, SntpConf, SyncStatus};
use esp_idf_svc::sys::EspError;
use tracing::info;

pub struct SystemClock {
    _sntp: EspSntp<'static>,
    booted_at: SystemTime,
    sntp_handle: SntpStatus,
}

struct SntpStatus;

impl SntpStatus {
    fn is_synced(&self) -> bool {
        matches!(
            SyncStatus::from(unsafe { esp_idf_svc::sys::sntp_get_sync_status() }),
            SyncStatus::Completed
        )
    }
}

impl SystemClock {
    pub fn new() -> Result<Self, EspError> {
        let sntp = EspSntp::new(&SntpConf::default())?;
        info!("SNTP started");

        Ok(Self {
            _sntp: sntp,
            booted_at: SystemTime::now(),
            sntp_handle: SntpStatus,
        })
    }

    fn civil(&self) -> Option<(u16, u8)> {
        let epoch = self.epoch_seconds()?;

        let local = unsafe {
            let t = epoch as esp_idf_svc::sys::time_t;
            let tm = esp_idf_svc::sys::localtime(&t);
            if tm.is_null() {
                return None;
            }
            *tm
        };

        let minute_of_day = (local.tm_hour as u16) * 60 + local.tm_min as u16;
        let month = (local.tm_mon as u8) + 1;

        Some((minute_of_day, month))
    }
}

impl Clock for SystemClock {
    fn epoch_seconds(&self) -> Option<u64> {
        if !self.sntp_handle.is_synced() {
            return None;
        }

        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|d| d.as_secs())
    }

    fn uptime_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(self.booted_at)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    }

    fn local_minute_of_day(&self) -> Option<u16> {
        self.civil().map(|(minute, _)| minute)
    }

    fn local_month(&self) -> Option<u8> {
        self.civil().map(|(_, month)| month)
    }
}
