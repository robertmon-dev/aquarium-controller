use std::cell::RefCell;
use std::rc::Rc;

use super::*;
use crate::domain::water::{WaterLevelRole, WaterLevelSnapshot, WaterLevelState};

struct FakePins {
    levels: Rc<RefCell<[bool; WaterLevelRole::COUNT]>>,
    reads: Rc<RefCell<usize>>,
}

impl WaterLevelPins for FakePins {
    fn read_raw(&mut self) -> [(WaterLevelRole, bool); WaterLevelRole::COUNT] {
        *self.reads.borrow_mut() += 1;
        let levels = *self.levels.borrow();
        WaterLevelRole::ALL.map(|role| (role, levels[role.index()]))
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
}

impl Harness {
    fn new() -> Self {
        Self {
            levels: Rc::new(RefCell::new([false; WaterLevelRole::COUNT])),
            reads: Rc::new(RefCell::new(0)),
        }
    }

    fn pins(&self) -> FakePins {
        FakePins {
            levels: Rc::clone(&self.levels),
            reads: Rc::clone(&self.reads),
        }
    }

    fn set(&self, role: WaterLevelRole, high: bool) {
        self.levels.borrow_mut()[role.index()] = high;
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
