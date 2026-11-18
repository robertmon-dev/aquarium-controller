use std::sync::Mutex;

use super::*;
use crate::domain::calibration::PhCalibration;

const PH: PhCalibration = PhCalibration {
    neutral_volts: 1.500,
    acid_volts: 1.67748,
    calibrated_at_celsius: 25.0,
};

fn defaults() -> Calibration {
    Calibration {
        ph: PH,
        makeup_tank_celsius: 25.0,
        tds_factor: 1.0,
    }
}

#[derive(Default)]
struct FakeStore {
    loaded: Option<Calibration>,
    written: Mutex<Vec<Calibration>>,
    fails: bool,
}

impl FakeStore {
    fn empty() -> Self {
        Self::default()
    }

    fn holding(value: Calibration) -> Self {
        Self {
            loaded: Some(value),
            ..Self::default()
        }
    }

    fn failing() -> Self {
        Self {
            fails: true,
            ..Self::default()
        }
    }

    fn writes(&self) -> Vec<Calibration> {
        self.written.lock().expect("not poisoned").clone()
    }
}

impl CalibrationPersistence for &FakeStore {
    fn load(&self) -> Option<Calibration> {
        self.loaded
    }

    fn store(&self, value: &Calibration) -> Result<(), &'static str> {
        if self.fails {
            return Err("fake: the flash is worn out");
        }

        self.written.lock().expect("not poisoned").push(*value);
        Ok(())
    }
}

fn patch_temperature(celsius: f32) -> CalibrationPatch {
    CalibrationPatch {
        ph: None,
        makeup_tank_celsius: Some(celsius),
        tds_factor: None,
    }
}

#[test]
fn an_empty_store_falls_back_to_the_defaults() {
    let store = FakeStore::empty();
    let service = CalibrationService::new(defaults(), &store);

    assert_eq!(service.current(), defaults());
}

#[test]
fn a_stored_calibration_wins_over_the_defaults() {
    let mut stored = defaults();
    stored.makeup_tank_celsius = 19.0;
    stored.tds_factor = 1.35;

    let store = FakeStore::holding(stored);
    let service = CalibrationService::new(defaults(), &store);

    assert_eq!(service.current(), stored);
}

#[test]
fn applying_a_patch_updates_the_reading_and_writes_it_through() {
    let store = FakeStore::empty();
    let service = CalibrationService::new(defaults(), &store);

    let applied = service
        .apply(patch_temperature(23.0))
        .expect("a sane temperature is accepted");

    assert_eq!(applied.makeup_tank_celsius, 23.0);
    assert_eq!(service.current().makeup_tank_celsius, 23.0);
    assert_eq!(store.writes().len(), 1);
    assert_eq!(store.writes()[0].makeup_tank_celsius, 23.0);
}

#[test]
fn an_invalid_patch_is_rejected_without_touching_the_store() {
    let store = FakeStore::empty();
    let service = CalibrationService::new(defaults(), &store);

    let rejected = service.apply(patch_temperature(80.0));

    assert!(matches!(rejected, Err(CalibrationError::Invalid(_))));
    assert_eq!(service.current(), defaults());
    assert!(store.writes().is_empty());
}

#[test]
fn a_failed_write_leaves_the_calibration_unchanged() {
    let store = FakeStore::failing();
    let service = CalibrationService::new(defaults(), &store);

    let rejected = service.apply(patch_temperature(23.0));

    assert!(matches!(rejected, Err(CalibrationError::NotPersisted(_))));
    assert_eq!(
        service.current(),
        defaults(),
        "a calibration that cannot survive a reboot must not be applied"
    );
}

#[test]
fn a_patch_replacing_the_ph_points_is_validated_as_a_whole() {
    let store = FakeStore::empty();
    let service = CalibrationService::new(defaults(), &store);

    let dead = CalibrationPatch {
        ph: Some(PhCalibration {
            neutral_volts: 1.5,
            acid_volts: 1.5,
            calibrated_at_celsius: 25.0,
        }),
        makeup_tank_celsius: None,
        tds_factor: None,
    };

    assert!(matches!(
        service.apply(dead),
        Err(CalibrationError::Invalid(_))
    ));
}

#[test]
fn the_service_reads_back_as_a_calibration_source() {
    let store = FakeStore::empty();
    let service = CalibrationService::new(defaults(), &store);

    service.apply(patch_temperature(21.0)).expect("accepted");

    assert_eq!(
        CalibrationSource::calibration(&service).makeup_tank_celsius,
        21.0
    );
}
