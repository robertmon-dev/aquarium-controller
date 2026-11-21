#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    OperationKind, OperationRecord, OperationResult, OperationState, PumpId, PumpSettingsPatch,
    PumpsStatus, SkippedWindow,
};
use aquarium_core::ports::inbound::PumpControl;

use sim::{civil, Event, Layout, Options, Sim, BASE_EPOCH};

const RESERVOIR_L: f64 = 150.0;
const BALANCE_TOL: f64 = 1e-6;
const OVERRIDE_S: u32 = 3600;
const JUMPS_S: [i64; 4] = [300, -300, 7200, -7200];
const WATER_CHANGE_AT_S: u64 = 60;
const TOP_OFF_AT_S: u64 = 1200;
const IDLE_JUMP_AT_S: f64 = 720.0;
const HORIZON_S: f64 = 3.0 * 3600.0;
const DEFICIT_L: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum When {
    Idle,
    Draining,
    Settling,
    Refilling,
    TopOff,
}

impl When {
    const ALL: [Self; 5] = [
        Self::Idle,
        Self::Draining,
        Self::Settling,
        Self::Refilling,
        Self::TopOff,
    ];
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn cron_at(offset_s: u64) -> String {
    let at = civil(BASE_EPOCH + offset_s);
    format!("{} {} * * *", at.minute, at.hour)
}

fn options() -> Options {
    Options {
        step: Duration::from_millis(500),
        http: false,
        ..Options::COARSE.manual()
    }
}

async fn start(windows: bool) -> Sim {
    let sim = Sim::start_with(Layout::REAL.with_reservoir(RESERVOIR_L), options()).await;
    let schedule = if windows {
        format!(
            r#","water_change":[{{"cron":"{}","drain_s":112,"settle_s":30,"refill_s":600}}],
               "top_off":[{{"cron":"{}","duration_s":120}}]"#,
            cron_at(WATER_CHANGE_AT_S),
            cron_at(TOP_OFF_AT_S)
        )
    } else {
        String::new()
    };
    let patch: PumpSettingsPatch = serde_json::from_str(&format!(
        r#"{{"flow_lph":{{"makeup":600,"drain":1500}},"refill_stop_sensor":"sump_return_level"{schedule}}}"#
    ))
    .expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");
    sim
}

#[derive(Default)]
struct Seen {
    water_changes: Vec<OperationRecord>,
    top_offs: Vec<OperationRecord>,
    skipped: Vec<SkippedWindow>,
}

impl Seen {
    fn note(&mut self, before: &PumpsStatus, after: &PumpsStatus) {
        if after.last_water_change != before.last_water_change {
            self.water_changes.extend(after.last_water_change);
        }
        if after.last_top_off != before.last_top_off {
            self.top_offs.extend(after.last_top_off);
        }
        if after.last_skipped != before.last_skipped {
            self.skipped.extend(after.last_skipped);
        }
    }
}

fn triggered(when: When, status: &PumpsStatus) -> bool {
    let operation = status.operation;
    match when {
        When::Idle => false,
        When::Draining => {
            operation.state == OperationState::Draining && operation.drain_ran_s >= 30
        }
        When::Settling => {
            operation.state == OperationState::Settling && operation.settle_ran_s >= 10
        }
        When::Refilling => {
            operation.state == OperationState::Refilling
                && operation.kind == Some(OperationKind::WaterChange)
                && operation.refill_ran_s >= 20
        }
        When::TopOff => {
            operation.state == OperationState::Refilling
                && operation.kind == Some(OperationKind::TopOff)
                && operation.refill_ran_s >= 5
        }
    }
}

async fn run(when: When, jump_s: i64) -> (Sim, Seen, f64) {
    let sim = start(true).await;
    let mut seen = Seen::default();
    let mut last = sim.service.status();
    let mut jumped_at = None;
    let mut deficit_added = false;

    while sim.water().t_s < HORIZON_S {
        let t = sim.water().t_s;

        if when == When::TopOff
            && !deficit_added
            && last.last_water_change.is_some()
            && last.operation.state == OperationState::Idle
        {
            sim.water().evaporate(DEFICIT_L);
            sim.run_for(secs(2)).await;
            sim.service.start_top_off(300).expect("top-off started");
            deficit_added = true;
        }

        let due = match when {
            When::Idle => t >= IDLE_JUMP_AT_S,
            _ => triggered(when, &last),
        };
        if jumped_at.is_none() && due {
            sim.clock.step_clock(jump_s);
            jumped_at = Some(t);
        }

        sim.step().await;
        let status = sim.service.status();
        seen.note(&last, &status);
        last = status;
    }

    let jumped_at = jumped_at.unwrap_or_else(|| panic!("{when:?} {jump_s:+}: never jumped"));
    (sim, seen, jumped_at)
}

#[tokio::test(start_paused = true)]
async fn clock_jumps_never_repeat_a_water_change_or_strand_an_operation() {
    LocalSet::new()
        .run_until(async {
            for when in When::ALL {
                for jump_s in JUMPS_S {
                    let label = format!("{when:?} {jump_s:+} s");
                    let (sim, seen, jumped_at) = run(when, jump_s).await;
                    let status = sim.service.status();
                    let water = sim.water();

                    println!(
                        "{label}: jumped at {jumped_at:.0} s, water changes {:?}, top-offs {:?}, skipped {:?}",
                        seen.water_changes
                            .iter()
                            .map(|record| (record.result, record.drain_ran_s, record.settle_ran_s))
                            .collect::<Vec<_>>(),
                        seen.top_offs.iter().map(|record| record.result).collect::<Vec<_>>(),
                        seen.skipped
                            .iter()
                            .map(|skip| (skip.kind, skip.reason, skip.blocked_by))
                            .collect::<Vec<_>>(),
                    );

                    assert_eq!(status.operation.state, OperationState::Idle, "{label}");
                    assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 1, "{label}");
                    assert_eq!(seen.water_changes.len(), 1, "{label}");
                    let change = seen.water_changes[0];
                    assert_eq!(change.result, OperationResult::SumpFull, "{label}");
                    assert_eq!(change.drain_ran_s, 112, "{label}");
                    assert_eq!(change.settle_ran_s, 30, "{label}");
                    assert!(
                        seen.top_offs
                            .iter()
                            .all(|record| record.result == OperationResult::SumpFull),
                        "{label}: {:?}",
                        seen.top_offs
                    );

                    let api_top_offs = usize::from(when == When::TopOff);
                    let window_attempts = seen.top_offs.len() - api_top_offs
                        + seen
                            .skipped
                            .iter()
                            .filter(|skip| skip.kind == OperationKind::TopOff)
                            .count();
                    assert_eq!(
                        window_attempts,
                        usize::from(jump_s != 7200),
                        "{label}: the top-off window fires once, or is lost when skipped over"
                    );
                    assert!(
                        seen.skipped
                            .iter()
                            .all(|skip| skip.kind == OperationKind::TopOff),
                        "{label}: {:?}",
                        seen.skipped
                    );

                    assert_eq!(water.spilled_l, 0.0, "{label}");
                    assert_eq!(water.return_dry_s, 0.0, "{label}");
                    assert_eq!(water.makeup_dry_s, 0.0, "{label}");
                    assert!(water.deficit_l().abs() < 1.0, "{label}: {}", water.describe());
                    assert!(water.balance_error_l().abs() < BALANCE_TOL, "{label}");
                }
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn override_expiry_follows_the_requested_duration_across_clock_jumps() {
    LocalSet::new()
        .run_until(async {
            for jump_s in JUMPS_S {
                for id in [PumpId::Return, PumpId::Uv] {
                    let label = format!("{id:?} {jump_s:+} s");
                    let sim = start(false).await;
                    sim.run_for(secs(10)).await;
                    let set_at = sim.water().t_s;
                    sim.service
                        .set_override(id, Some(OVERRIDE_S))
                        .expect("override set");
                    sim.run_for(secs(600)).await;
                    sim.clock.step_clock(jump_s);
                    sim.run_for(secs(2)).await;

                    let remaining = sim.service.status().channels.get(id).override_expires_in_s;
                    println!("{label}: {remaining:?} s left after the jump");
                    assert!(
                        remaining.is_some_and(|left| left <= OVERRIDE_S - 600),
                        "{label}: {remaining:?}"
                    );

                    let back_on = sim
                        .run_until(secs(3 * 3600), "the override to expire", |_, water| {
                            water.pump_on(id)
                        })
                        .await;
                    let lasted = back_on - set_at;
                    println!("{label}: the override lasted {lasted:.1} s");
                    assert!(
                        (f64::from(OVERRIDE_S) - 1.0..=f64::from(OVERRIDE_S) + 2.0)
                            .contains(&lasted),
                        "{label}: lasted {lasted:.1} s"
                    );
                }
            }
        })
        .await;
}
