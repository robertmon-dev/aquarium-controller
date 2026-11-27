use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

struct FakePins {
    levels: Rc<RefCell<[bool; WaterLevelRole::COUNT]>>,
    reads: Rc<RefCell<usize>>,
    failing: Rc<RefCell<bool>>,
}

impl WaterLevelPins for FakePins {
    fn read_raw(
        &mut self,
    ) -> Result<[(WaterLevelRole, bool); WaterLevelRole::COUNT], &'static str> {
        *self.reads.borrow_mut() += 1;
        if *self.failing.borrow() {
            return Err("bus down");
        }
        let levels = *self.levels.borrow();
        Ok(WaterLevelRole::ALL.map(|role| (role, levels[role.index()])))
    }
}

#[derive(Default)]
struct RecordingSink {
    published: RefCell<Vec<WaterLevelSnapshot>>,
}

impl WaterLevelSink for &RecordingSink {
    fn publish(&self, snapshot: WaterLevelSnapshot) {
        self.published.borrow_mut().push(snapshot);
    }
}

const IMMEDIATE: FilterConfig = FilterConfig {
    debounce_samples: 1,
    flap_window_samples: 1_000,
    flap_max_transitions: u8::MAX,
};

const DEBOUNCE_THREE: FilterConfig = FilterConfig {
    debounce_samples: 3,
    flap_window_samples: 1_000,
    flap_max_transitions: u8::MAX,
};

struct Harness {
    levels: Rc<RefCell<[bool; WaterLevelRole::COUNT]>>,
    reads: Rc<RefCell<usize>>,
    failing: Rc<RefCell<bool>>,
}

impl Harness {
    fn new() -> Self {
        Self {
            levels: Rc::new(RefCell::new([false; WaterLevelRole::COUNT])),
            reads: Rc::new(RefCell::new(0)),
            failing: Rc::new(RefCell::new(false)),
        }
    }

    fn pins(&self) -> FakePins {
        FakePins {
            levels: Rc::clone(&self.levels),
            reads: Rc::clone(&self.reads),
            failing: Rc::clone(&self.failing),
        }
    }

    fn set(&self, role: WaterLevelRole, high: bool) {
        self.levels.borrow_mut()[role.index()] = high;
    }

    fn fail(&self, failing: bool) {
        *self.failing.borrow_mut() = failing;
    }
}

#[test]
fn every_tick_publishes_a_snapshot() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, IMMEDIATE);

    sampler.tick();
    sampler.tick();
    sampler.tick();

    assert_eq!(sink.published.borrow().len(), 3);
    assert_eq!(*harness.reads.borrow(), 3, "each tick reads the pins once");
}

#[test]
fn each_role_carries_its_own_level() {
    let harness = Harness::new();
    harness.set(WaterLevelRole::SumpPump, true);
    harness.set(WaterLevelRole::MainTank, true);

    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, IMMEDIATE);

    sampler.tick();

    let snapshot = sink.published.borrow().last().cloned().expect("published");
    assert_eq!(
        snapshot.get(WaterLevelRole::SumpPump).state,
        WaterLevelState::Wet
    );
    assert_eq!(
        snapshot.get(WaterLevelRole::MainTank).state,
        WaterLevelState::Wet
    );
    assert_eq!(
        snapshot.get(WaterLevelRole::SumpMechanical).state,
        WaterLevelState::Dry
    );
    assert_eq!(
        snapshot.get(WaterLevelRole::SumpBiological).state,
        WaterLevelState::Dry
    );
}

#[test]
fn each_reading_is_tagged_with_the_role_it_describes() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, IMMEDIATE);

    sampler.tick();

    let snapshot = sink.published.borrow().last().cloned().expect("published");
    for role in WaterLevelRole::ALL {
        assert_eq!(
            snapshot.get(role).role,
            role,
            "readings must not be shuffled between roles"
        );
    }
}

#[test]
fn the_published_level_is_filtered_not_raw() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, DEBOUNCE_THREE);

    for _ in 0..3 {
        sampler.tick();
    }

    harness.set(WaterLevelRole::SumpPump, true);
    sampler.tick();

    let snapshot = sink.published.borrow().last().cloned().expect("published");
    assert_eq!(
        snapshot.get(WaterLevelRole::SumpPump).state,
        WaterLevelState::Dry,
        "a single sample must not reach the published snapshot"
    );

    sampler.tick();
    sampler.tick();

    let snapshot = sink.published.borrow().last().cloned().expect("published");
    assert_eq!(
        snapshot.get(WaterLevelRole::SumpPump).state,
        WaterLevelState::Wet,
        "three agreeing samples must reach it"
    );
}

#[test]
fn polarity_is_applied_to_every_pin() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler = WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveLow, IMMEDIATE);

    sampler.tick();

    let snapshot = sink.published.borrow().last().cloned().expect("published");
    for role in WaterLevelRole::ALL {
        assert_eq!(
            snapshot.get(role).state,
            WaterLevelState::Wet,
            "a low pin reads as wet under active-low wiring"
        );
    }
}

#[test]
fn nothing_is_published_until_every_sensor_has_settled() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, DEBOUNCE_THREE);

    sampler.tick();
    sampler.tick();
    assert!(
        sink.published.borrow().is_empty(),
        "a boot must not publish the filters' placeholder level"
    );

    sampler.tick();
    assert_eq!(sink.published.borrow().len(), 1);
}

#[test]
fn a_sensor_wet_at_boot_is_published_wet_on_the_first_snapshot() {
    let harness = Harness::new();
    harness.set(WaterLevelRole::SumpPump, true);
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, DEBOUNCE_THREE);

    for _ in 0..3 {
        sampler.tick();
    }

    let published = sink.published.borrow();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].get(WaterLevelRole::SumpPump).state,
        WaterLevelState::Wet
    );
}

#[test]
fn a_sensor_that_glitches_at_boot_holds_back_the_first_snapshot() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, DEBOUNCE_THREE);

    harness.set(WaterLevelRole::MainTank, true);
    sampler.tick();
    harness.set(WaterLevelRole::MainTank, false);
    sampler.tick();
    sampler.tick();
    assert!(sink.published.borrow().is_empty());

    sampler.tick();
    let published = sink.published.borrow();
    assert_eq!(published.len(), 1);
    assert_eq!(
        published[0].get(WaterLevelRole::MainTank).state,
        WaterLevelState::Dry
    );
}

#[test]
fn an_implausible_pair_is_published_faulty() {
    let harness = Harness::new();
    harness.set(WaterLevelRole::SumpReturnLevel, true);
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, IMMEDIATE);

    sampler.tick();

    let snapshot = sink.published.borrow().last().cloned().expect("published");
    assert!(snapshot.get(WaterLevelRole::SumpReturnLevel).faulty);
    assert!(snapshot.get(WaterLevelRole::SumpPump).faulty);
    assert!(!snapshot.get(WaterLevelRole::MainTank).faulty);
}

#[test]
fn a_failed_read_publishes_nothing() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, IMMEDIATE);

    sampler.tick();
    harness.fail(true);
    sampler.tick();
    sampler.tick();

    assert_eq!(
        sink.published.borrow().len(),
        1,
        "a dead bus must let the last sample age out instead of refreshing it"
    );

    harness.fail(false);
    sampler.tick();

    assert_eq!(
        sink.published.borrow().len(),
        2,
        "publishing resumes with the bus"
    );
}

#[test]
fn a_failed_read_is_not_a_sample() {
    let harness = Harness::new();
    let sink = RecordingSink::default();
    let mut sampler =
        WaterLevelSampler::new(harness.pins(), &sink, Polarity::ActiveHigh, DEBOUNCE_THREE);

    sampler.tick();
    sampler.tick();
    harness.fail(true);
    sampler.tick();
    harness.fail(false);

    assert!(sink.published.borrow().is_empty());

    sampler.tick();
    assert_eq!(
        sink.published.borrow().len(),
        1,
        "only real readings count towards settling"
    );
}
