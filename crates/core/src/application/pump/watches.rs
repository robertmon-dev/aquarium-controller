use std::collections::VecDeque;
use std::time::Duration;

use tokio::time::Instant;

use crate::domain::pump::{LevelConditions, STARVED_CLEAR_S, STARVED_LOCKOUTS, STARVED_WINDOW_S};

#[derive(Debug, Default)]
pub(super) struct ReturnLockout {
    locked: bool,
    wet_since: Option<Instant>,
}

impl ReturnLockout {
    pub(super) fn update(
        &mut self,
        levels: Option<LevelConditions>,
        now: Instant,
        delay: Duration,
    ) -> bool {
        match levels {
            None => {
                self.wet_since = None;
                false
            }
            Some(levels) if levels.sump_low => {
                self.locked = true;
                self.wet_since = None;
                true
            }
            Some(_) if self.locked => {
                let since = *self.wet_since.get_or_insert(now);

                if now.saturating_duration_since(since) >= delay {
                    self.locked = false;
                    self.wet_since = None;
                }

                self.locked
            }
            Some(_) => false,
        }
    }
}

#[derive(Debug, Default)]
pub(super) struct CirculationWatch {
    lockouts: VecDeque<Instant>,
    low_since: Option<Instant>,
    wet_since: Option<Instant>,
    was_low: bool,
    starved: bool,
}

impl CirculationWatch {
    pub(super) fn update(&mut self, sump_low: Option<bool>, operating: bool, now: Instant) -> bool {
        let window = Duration::from_secs(STARVED_WINDOW_S);

        match sump_low {
            None => {
                self.low_since = None;
                self.wet_since = None;
                self.was_low = false;
            }
            Some(true) => {
                self.wet_since = None;

                if operating {
                    self.low_since = None;
                } else {
                    if !self.was_low {
                        self.lockouts.push_back(now);
                    }

                    let since = *self.low_since.get_or_insert(now);
                    self.lockouts
                        .retain(|at| now.saturating_duration_since(*at) < window);
                    self.starved |= self.lockouts.len() >= STARVED_LOCKOUTS
                        || now.saturating_duration_since(since) >= window;
                }

                self.was_low = true;
            }
            Some(false) => {
                self.low_since = None;
                let since = *self.wet_since.get_or_insert(now);

                if now.saturating_duration_since(since) >= Duration::from_secs(STARVED_CLEAR_S) {
                    self.starved = false;
                    self.lockouts.clear();
                }

                self.was_low = false;
            }
        }

        self.starved
    }
}

#[cfg(test)]
mod tests;
