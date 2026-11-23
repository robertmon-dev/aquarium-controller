use crate::domain::pump::{LevelConditions, Operation, OperationPlan, Step, WaterChangePlan};

pub(super) const HEALTHY: LevelConditions = LevelConditions {
    sump_low: false,
    reservoir_empty: false,
    reservoir_ready: true,
    tank_overflow: false,
    sump_full: false,
    sump_high: false,
};

pub(super) const SECOND: u64 = 1000;

pub(super) fn plan(drain_s: u32, settle_s: u32, refill_s: u32) -> OperationPlan {
    OperationPlan::water_change(WaterChangePlan {
        drain_s,
        settle_s,
        refill_s,
    })
}

pub(super) fn run(operation: &mut Operation, seconds: u64, levels: LevelConditions) -> Step {
    let mut last = Step::Continue;
    for _ in 0..seconds {
        last = operation.advance(SECOND, SECOND, Some(levels));
        if last != Step::Continue {
            return last;
        }
    }
    last
}
