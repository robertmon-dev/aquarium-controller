use crate::domain::water::WaterLevelState;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    ActiveHigh,
    ActiveLow,
}

pub fn interpret(pin_high: bool, polarity: Polarity) -> WaterLevelState {
    let wet = match polarity {
        Polarity::ActiveHigh => pin_high,
        Polarity::ActiveLow => !pin_high,
    };

    if wet {
        WaterLevelState::Wet
    } else {
        WaterLevelState::Dry
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterConfig {
    pub debounce_samples: u16,
    pub flap_window_samples: u16,
    pub flap_max_transitions: u8,
}

pub struct LevelFilter {
    config: FilterConfig,
    stable: WaterLevelState,
    agreed: u16,
    last_raw: WaterLevelState,
    transitions: u8,
    elapsed: u16,
    faulty: bool,
    unstable: bool,
    settled: bool,
    run: u16,
    seen: u16,
}

impl LevelFilter {
    pub fn new(initial: WaterLevelState, config: FilterConfig) -> Self {
        Self {
            config,
            stable: initial,
            agreed: 0,
            last_raw: initial,
            transitions: 0,
            elapsed: 0,
            faulty: false,
            unstable: false,
            settled: true,
            run: 0,
            seen: 0,
        }
    }

    pub fn unsettled(config: FilterConfig) -> Self {
        Self {
            settled: false,
            ..Self::new(WaterLevelState::Dry, config)
        }
    }

    pub fn push(&mut self, raw: WaterLevelState) -> bool {
        if !self.settled {
            return self.settle(raw);
        }

        self.count_run(raw);
        if self.run >= self.config.debounce_samples {
            self.unstable = false;
        }
        self.track_chatter(raw);

        if raw == self.stable {
            self.agreed = 0;
            return false;
        }

        self.agreed += 1;
        if self.agreed < self.config.debounce_samples {
            return false;
        }

        self.stable = raw;
        self.agreed = 0;
        true
    }

    pub fn state(&self) -> WaterLevelState {
        self.stable
    }

    pub fn is_faulty(&self) -> bool {
        self.faulty || self.unstable
    }

    pub fn is_settled(&self) -> bool {
        self.settled
    }

    fn count_run(&mut self, raw: WaterLevelState) {
        self.run = if raw == self.last_raw {
            self.run.saturating_add(1)
        } else {
            1
        };
    }

    fn settle(&mut self, raw: WaterLevelState) -> bool {
        self.count_run(raw);
        self.track_chatter(raw);
        self.seen = self.seen.saturating_add(1);

        let agreed = self.run >= self.config.debounce_samples;
        if !agreed && self.seen < self.config.flap_window_samples {
            return false;
        }

        self.settled = true;
        self.unstable = !agreed;
        if !agreed || raw == self.stable {
            return false;
        }

        self.stable = raw;
        true
    }

    fn track_chatter(&mut self, raw: WaterLevelState) {
        if raw != self.last_raw {
            self.transitions = self.transitions.saturating_add(1);
            self.last_raw = raw;
        }

        self.elapsed += 1;
        if self.elapsed < self.config.flap_window_samples {
            return;
        }

        self.faulty = self.transitions > self.config.flap_max_transitions;
        self.transitions = 0;
        self.elapsed = 0;
    }
}

#[cfg(test)]
mod tests;
