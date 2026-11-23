use super::conditions::ReservoirConditions;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RefillDebounce {
    dry_ms: Option<u64>,
}

impl RefillDebounce {
    pub fn advance(&mut self, conditions: Option<ReservoirConditions>, elapsed_ms: u64) {
        self.dry_ms = match conditions {
            Some(conditions) if !conditions.full && !conditions.full_faulty => {
                Some(self.dry_ms.map_or(0, |dry| dry.saturating_add(elapsed_ms)))
            }
            _ => None,
        };
    }

    pub const fn due(&self, debounce_s: u32) -> bool {
        matches!(self.dry_ms, Some(ms) if ms >= debounce_s as u64 * 1000)
    }

    pub fn reset(&mut self) {
        self.dry_ms = self.dry_ms.map(|_| 0);
    }
}

#[cfg(test)]
mod tests;
