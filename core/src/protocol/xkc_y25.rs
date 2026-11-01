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
        }
    }

    pub fn push(&mut self, raw: WaterLevelState) -> bool {
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
        self.faulty
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
