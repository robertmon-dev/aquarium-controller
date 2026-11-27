#[cfg(target_os = "espidf")]
pub use hw::RgbwOutputs;

#[cfg(target_os = "espidf")]
mod hw {
    use aquarium_core::domain::lighting::ChannelLevels;
    use aquarium_core::ports::outbound::LightingOutputs;

    use super::super::pwm::LedcBank;

    pub struct RgbwOutputs<'d>(LedcBank<'d, 4>);

    impl<'d> RgbwOutputs<'d> {
        pub fn new(bank: LedcBank<'d, 4>) -> Self {
            Self(bank)
        }
    }

    impl LightingOutputs for RgbwOutputs<'_> {
        fn set_levels(&mut self, levels: ChannelLevels) -> Result<(), &'static str> {
            let channels = [
                levels.red_pct,
                levels.green_pct,
                levels.blue_pct,
                levels.white_pct,
            ];

            for (index, pct) in channels.into_iter().enumerate() {
                self.0.set_pct(index, pct)?;
            }

            Ok(())
        }
    }
}
