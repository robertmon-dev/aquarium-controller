use super::conditions::ReservoirConditions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefillDebounce {
    dry_ms: u64,
}

impl RefillDebounce {
    pub fn advance(&mut self, conditions: Option<ReservoirConditions>, elapsed_ms: u64) {
        self.dry_ms = match conditions {
            Some(conditions) if !conditions.full => self.dry_ms.saturating_add(elapsed_ms),
            _ => 0,
        };
    }

    pub const fn due(&self, debounce_s: u32) -> bool {
        self.dry_ms >= debounce_s as u64 * 1000
    }

    pub fn reset(&mut self) {
        self.dry_ms = 0;
    }
}

#[cfg(test)]
mod tests;
