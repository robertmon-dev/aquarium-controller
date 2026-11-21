use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use aquarium_core::domain::cron::CivilTime;
use aquarium_core::ports::outbound::Clock;
use esp_idf_svc::sntp::{EspSntp, SntpConf, SyncStatus};
use esp_idf_svc::sys::EspError;
use tracing::info;

extern "C" {
    fn tzset();
}

pub struct SystemClock {
    _sntp: Option<EspSntp<'static>>,
    booted_at: SystemTime,
    sntp_handle: SntpStatus,
    sntp_enabled: bool,
}

#[derive(Default)]
struct SntpStatus {
    synced: AtomicBool,
}

impl SntpStatus {
    fn is_synced(&self) -> bool {
        if self.synced.load(Ordering::Relaxed) {
            return true;
        }

        let completed = matches!(
            SyncStatus::from(unsafe { esp_idf_svc::sys::sntp_get_sync_status() }),
            SyncStatus::Completed
        );

        if completed {
            self.synced.store(true, Ordering::Relaxed);
            info!("SNTP time synchronized");
        }

        completed
    }
}

impl SystemClock {
    pub fn new(timezone: &str, start_sntp: bool) -> Result<Self, EspError> {
        std::env::set_var("TZ", timezone);
        unsafe { tzset() };

        let sntp = if start_sntp {
            let sntp = EspSntp::new(&SntpConf::default())?;
            info!(timezone, "SNTP started");
            Some(sntp)
        } else {
            info!(timezone, "SNTP not started: WiFi is not initialized");
            None
        };

        Ok(Self {
            _sntp: sntp,
            booted_at: SystemTime::now(),
            sntp_handle: SntpStatus::default(),
            sntp_enabled: start_sntp,
        })
    }

    fn local_tm(&self) -> Option<esp_idf_svc::sys::tm> {
        let epoch = self.epoch_seconds()?;

        unsafe {
            let t = epoch as esp_idf_svc::sys::time_t;
            let tm = esp_idf_svc::sys::localtime(&t);
            if tm.is_null() {
                return None;
            }
            Some(*tm)
        }
    }
}

impl Clock for SystemClock {
    fn epoch_seconds(&self) -> Option<u64> {
        if !self.sntp_enabled || !self.sntp_handle.is_synced() {
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
        self.local_tm()
            .map(|tm| (tm.tm_hour as u16) * 60 + tm.tm_min as u16)
    }

    fn local_month(&self) -> Option<u8> {
        self.local_tm().map(|tm| (tm.tm_mon as u8) + 1)
    }

    fn local_civil_time(&self) -> Option<CivilTime> {
        self.local_tm().map(|tm| CivilTime {
            minute: tm.tm_min as u8,
            hour: tm.tm_hour as u8,
            day: tm.tm_mday as u8,
            month: (tm.tm_mon as u8) + 1,
            weekday: tm.tm_wday as u8,
        })
    }
}
