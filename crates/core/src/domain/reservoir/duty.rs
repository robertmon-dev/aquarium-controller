#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DutyLimits {
    pub max_on_ms: u64,
    pub rest_ms: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DutyMeter {
    worked_ms: u64,
    idle_ms: u64,
    rest_left_ms: u64,
}

impl DutyMeter {
    pub fn advance(&mut self, ran: bool, elapsed_ms: u64, limits: DutyLimits) {
        if self.rest_left_ms > 0 {
            self.rest_left_ms = self.rest_left_ms.saturating_sub(elapsed_ms);
            if self.rest_left_ms == 0 {
                self.worked_ms = 0;
                self.idle_ms = 0;
            }
            return;
        }

        if ran {
            self.worked_ms += elapsed_ms;
            self.idle_ms = 0;
            if self.worked_ms >= limits.max_on_ms {
                self.rest_left_ms = limits.rest_ms;
            }
        } else {
            self.idle_ms += elapsed_ms;
            if self.idle_ms >= limits.rest_ms {
                self.worked_ms = 0;
            }
        }
    }

    pub const fn resting(&self) -> bool {
        self.rest_left_ms > 0
    }

    pub const fn rest_remaining_s(&self) -> Option<u32> {
        if self.rest_left_ms == 0 {
            None
        } else {
            Some(self.rest_left_ms.div_ceil(1000) as u32)
        }
    }
}

#[cfg(test)]
mod tests;
