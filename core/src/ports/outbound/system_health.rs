pub trait SystemHealthProvider {
    fn wifi_rssi_dbm(&self) -> i8;
    fn free_heap_kb(&self) -> u32;
}
