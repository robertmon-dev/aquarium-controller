use std::sync::{Arc, Mutex};

use crate::domain::calibration::{Calibration, CalibrationPatch};
use crate::ports::inbound::{CalibrationControl, CalibrationError};
use crate::ports::outbound::{CalibrationPersistence, CalibrationSource};

pub struct CalibrationService<P> {
    state: Mutex<Calibration>,
    persistence: P,
}

impl<P: CalibrationPersistence> CalibrationService<P> {
    pub fn new(defaults: Calibration, persistence: P) -> Self {
        let restored = persistence.load().unwrap_or(defaults);

        Self {
            state: Mutex::new(restored),
            persistence,
        }
    }
}

impl<P: CalibrationPersistence> CalibrationControl for CalibrationService<P> {
    fn current(&self) -> Calibration {
        *self.state.lock().expect("calibration mutex poisoned")
    }

    fn apply(&self, patch: CalibrationPatch) -> Result<Calibration, CalibrationError> {
        let mut state = self.state.lock().expect("calibration mutex poisoned");
        let candidate = patch.merge_onto(*state);

        candidate.validate().map_err(CalibrationError::Invalid)?;
        self.persistence
            .store(&candidate)
            .map_err(CalibrationError::NotPersisted)?;

        *state = candidate;

        Ok(candidate)
    }
}

impl<P: CalibrationPersistence> CalibrationSource for CalibrationService<P> {
    fn calibration(&self) -> Calibration {
        self.current()
    }
}

impl<P: CalibrationPersistence> CalibrationControl for Arc<CalibrationService<P>> {
    fn current(&self) -> Calibration {
        self.as_ref().current()
    }

    fn apply(&self, patch: CalibrationPatch) -> Result<Calibration, CalibrationError> {
        self.as_ref().apply(patch)
    }
}

impl<P: CalibrationPersistence> CalibrationSource for Arc<CalibrationService<P>> {
    fn calibration(&self) -> Calibration {
        self.as_ref().current()
    }
}

#[cfg(test)]
mod tests;
