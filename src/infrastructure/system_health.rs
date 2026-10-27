use aquarium_core::ports::outbound::SystemHealthProvider;
use esp_idf_svc::sys::{esp_get_free_heap_size, esp_wifi_sta_get_rssi, ESP_OK};

pub struct EspSystemHealth;

impl SystemHealthProvider for EspSystemHealth {
    fn wifi_rssi_dbm(&self) -> i8 {
        let mut rssi: core::ffi::c_int = 0;

        let status = unsafe { esp_wifi_sta_get_rssi(&mut rssi as *mut core::ffi::c_int) };

        if status != ESP_OK {
            return 0;
        }

        rssi as i8
    }

    fn free_heap_kb(&self) -> u32 {
        (unsafe { esp_get_free_heap_size() }) / 1024
    }
}
