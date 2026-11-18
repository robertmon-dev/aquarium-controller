pub trait ProbeVoltageSink {
    fn publish_volts(&self, ph: Option<f32>, tds: Option<f32>, sampled_at_epoch_s: u64);
}
