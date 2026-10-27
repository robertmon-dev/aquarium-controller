pub trait Clock {
    fn epoch_seconds(&self) -> Option<u64>;
    fn uptime_seconds(&self) -> u64;
    fn local_minute_of_day(&self) -> Option<u16>;
    fn local_month(&self) -> Option<u8>;
}
