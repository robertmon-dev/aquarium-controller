use crate::domain::water::{
    WaterLevelReading, WaterLevelRole, WaterLevelSnapshot, WaterLevelState,
};
use crate::ports::outbound::water_level::{WaterLevelPins, WaterLevelSink};
use crate::protocol::xkc_y25::{interpret, FilterConfig, LevelFilter, Polarity};

pub struct WaterLevelSampler<P, S> {
    pins: P,
    sink: S,
    polarity: Polarity,
    filters: [LevelFilter; WaterLevelRole::COUNT],
}

impl<P, S> WaterLevelSampler<P, S>
where
    P: WaterLevelPins,
    S: WaterLevelSink,
{
    pub fn new(pins: P, sink: S, polarity: Polarity, config: FilterConfig) -> Self {
        Self {
            pins,
            sink,
            polarity,
            filters: WaterLevelRole::ALL.map(|_| LevelFilter::new(WaterLevelState::Dry, config)),
        }
    }

    pub fn tick(&mut self) {
        let mut snapshot = WaterLevelSnapshot::dry();

        for (role, pin_high) in self.pins.read_raw() {
            let filter = &mut self.filters[role.index()];
            filter.push(interpret(pin_high, self.polarity));

            snapshot.sensors[role.index()] = WaterLevelReading {
                role,
                state: filter.state(),
                faulty: filter.is_faulty(),
            };
        }

        self.sink.publish(snapshot);
    }
}

#[cfg(test)]
mod tests;
