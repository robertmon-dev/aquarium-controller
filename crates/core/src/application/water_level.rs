use crate::domain::water::{WaterLevelReading, WaterLevelRole, WaterLevelSnapshot};
use crate::ports::outbound::water_level::{WaterLevelPins, WaterLevelSink};
use crate::protocol::xkc_y25::{interpret, FilterConfig, LevelFilter, Polarity};

pub struct WaterLevelSampler<P, S> {
    pins: P,
    sink: S,
    polarity: Polarity,
    filters: [LevelFilter; WaterLevelRole::COUNT],
    read_failing: bool,
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
            filters: WaterLevelRole::ALL.map(|_| LevelFilter::unsettled(config)),
            read_failing: false,
        }
    }

    pub fn tick(&mut self) {
        let raw = match self.pins.read_raw() {
            Ok(raw) => {
                if self.read_failing {
                    tracing::info!("water level inputs readable again");
                    self.read_failing = false;
                }
                raw
            }
            Err(error) => {
                if !self.read_failing {
                    tracing::warn!(%error, "water level inputs unreadable");
                    self.read_failing = true;
                }
                return;
            }
        };

        let mut snapshot = WaterLevelSnapshot::dry();

        for (role, pin_high) in raw {
            let filter = &mut self.filters[role.index()];
            filter.push(interpret(pin_high, self.polarity));

            snapshot.sensors[role.index()] = WaterLevelReading {
                role,
                state: filter.state(),
                faulty: filter.is_faulty(),
            };
        }

        if self.filters.iter().all(LevelFilter::is_settled) {
            self.sink.publish(snapshot.with_plausibility());
        }
    }
}

#[cfg(test)]
mod tests;
