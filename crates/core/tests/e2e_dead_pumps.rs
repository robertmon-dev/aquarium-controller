#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    Blocker, ChannelFault, OperationRecord, OperationResult, OperationState, PumpId,
    PumpSettingsPatch, WaterChangePlan,
};
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::PumpStatusSource;

use sim::{Layout, Sim};

const RESERVOIR_L: f64 = 150.0;
const BALANCE_TOL: f64 = 1e-6;
const PLAN: WaterChangePlan = WaterChangePlan {
    drain_s: 112,
    settle_s: 30,
    refill_s: 600,
};
const LONG: Duration = Duration::from_secs(3600);
const STARVED_AFTER: Duration = Duration::from_secs(660);

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

async fn start(dead: PumpId) -> Sim {
    start_stopping_at(Some(dead), "sump_return_level").await
}

#[tokio::test(start_paused = true)]
async fn followup_drain_waits_for_the_return_pump_to_switch_off() {
    LocalSet::new()
        .run_until(async {
            let sim = start_stopping_at(None, "sump_return_level").await;
            assert!(sim.water().pump_on(PumpId::Return));
            sim.relays.refuse_off(Some(PumpId::Return));
            sim.service.start_water_change(PLAN).expect("accepted");
            sim.run_for(secs(5)).await;

            let water = sim.water();
            assert!(water.pump_on(PumpId::Return));
            assert_eq!(
                water.drained_l, 0.0,
                "the drain removed water while the return relay refused OFF"
            );
        })
        .await;
}

async fn start_stopping_at(dead: Option<PumpId>, stop_sensor: &str) -> Sim {
    let sim = Sim::start(Layout::REAL.with_reservoir(RESERVOIR_L)).await;
    let patch: PumpSettingsPatch = serde_json::from_str(&format!(
        r#"{{"flow_lph":{{"makeup":600,"drain":1500}},"refill_stop_sensor":"{stop_sensor}"}}"#
    ))
    .expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");
    sim.run_for(secs(5)).await;
    if let Some(dead) = dead {
        sim.water().dead[dead.index()] = true;
    }
    sim
}

async fn water_change(sim: &Sim) -> OperationRecord {
    water_change_with(sim, PLAN).await
}

async fn water_change_with(sim: &Sim, plan: WaterChangePlan) -> OperationRecord {
    sim.service
        .start_water_change(plan)
        .expect("water change started");
    sim.run_until(LONG, "the water change to finish", |status, _| {
        status.operation.state == OperationState::Idle && status.last_water_change.is_some()
    })
    .await;
    sim.service
        .status()
        .last_water_change
        .expect("a finished water change")
}

fn report(label: &str, sim: &Sim, record: &OperationRecord) {
    let status = sim.service.status();
    println!(
        "{label}: {:?} drain {} s refill {} s, starved {}, {}",
        record.result,
        record.drain_ran_s,
        record.refill_ran_s,
        status.circulation_starved,
        sim.water().describe()
    );
}

#[tokio::test(start_paused = true)]
async fn a_dead_makeup_pump_ends_incomplete_and_starves_the_circulation() {
    LocalSet::new()
        .run_until(async {
            let sim = start(PumpId::Makeup).await;
            let record = water_change(&sim).await;
            report("dead makeup", &sim, &record);

            assert_eq!(record.result, OperationResult::Incomplete);
            assert_eq!(record.drain_ran_s, PLAN.drain_s);
            assert_eq!(record.refill_ran_s, PLAN.refill_s);
            {
                let water = sim.water();
                assert_eq!(water.added_l, 0.0);
                assert!(water.deficit_l() > 40.0, "{}", water.describe());
                assert_eq!(water.return_dry_s, 0.0);
                assert_eq!(water.spilled_l, 0.0);
                assert!(water.balance_error_l().abs() < BALANCE_TOL);
            }

            let status = sim.service.status();
            assert_eq!(
                status.channels.return_pump.blocked_by,
                Some(Blocker::SumpLow)
            );
            assert!(!status.circulation_starved);

            sim.run_for(STARVED_AFTER).await;
            let status = sim.service.status();
            assert!(status.circulation_starved);
            assert!(sim.service.pump_telemetry().circulation_starved);
            assert!(!sim.water().pump_on(PumpId::Return));

            sim.water().dead[PumpId::Makeup.index()] = false;
            sim.service.start_top_off(600).expect("top-off started");
            sim.run_until(LONG, "the top-off to refill the system", |status, _| {
                status.operation.state == OperationState::Idle && status.last_top_off.is_some()
            })
            .await;
            assert_eq!(
                sim.service
                    .status()
                    .last_top_off
                    .map(|record| record.result),
                Some(OperationResult::SumpFull)
            );
            assert!(sim.service.status().circulation_starved);

            sim.run_for(STARVED_AFTER).await;
            assert!(!sim.service.status().circulation_starved);
            assert!(sim.water().deficit_l().abs() < 1.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_return_pump_never_arms_the_refill_at_the_return_level() {
    LocalSet::new()
        .run_until(async {
            let sim = start(PumpId::Return).await;
            let record = water_change(&sim).await;
            report("dead return, stop at sump_return_level", &sim, &record);

            assert_eq!(record.result, OperationResult::RefillTimeout);
            assert_eq!(record.drain_ran_s, PLAN.drain_s);
            assert_eq!(record.refill_ran_s, 0);
            {
                let water = sim.water();
                assert_eq!(water.added_l, 0.0);
                assert!((water.deficit_l() - water.drained_l).abs() < 1e-9);
                assert!(water.drained_l > 40.0, "{}", water.describe());
                assert_eq!(water.spilled_l, 0.0);
                assert!(water.balance_error_l().abs() < BALANCE_TOL);
            }

            sim.run_for(STARVED_AFTER).await;
            let status = sim.service.status();
            assert!(
                !status.circulation_starved,
                "a dead return pump keeps the sump full, so the sump sensors cannot see it"
            );
            assert!(status.channels.return_pump.on);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_return_pump_under_a_high_stop_hides_any_drain_in_the_sump() {
    LocalSet::new()
        .run_until(async {
            for (drain_s, expected) in [
                (PLAN.drain_s, OperationResult::SumpFull),
                (200, OperationResult::SumpFull),
            ] {
                let sim = start_stopping_at(Some(PumpId::Return), "sump_mechanical").await;
                let record = water_change_with(&sim, WaterChangePlan { drain_s, ..PLAN }).await;
                report(
                    &format!("dead return, stop at sump_mechanical, drain {drain_s} s"),
                    &sim,
                    &record,
                );

                assert_eq!(record.result, expected, "drain {drain_s} s");
                assert_eq!(record.drain_ran_s, drain_s);
                let water = sim.water();
                let layout = water.layout;
                assert!(
                    (water.added_l - layout.headroom_to_level_l(layout.sump_mechanical.height_cm))
                        .abs()
                        < 0.6,
                    "{}",
                    water.describe()
                );
                assert!(water.deficit_l() > 10.0, "{}", water.describe());
                assert_eq!(water.spilled_l, 0.0);
                assert!(water.balance_error_l().abs() < BALANCE_TOL);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_dead_drain_pump_moves_nothing_and_times_out_the_refill() {
    LocalSet::new()
        .run_until(async {
            let sim = start(PumpId::Drain).await;
            let record = water_change(&sim).await;
            report("dead drain", &sim, &record);

            assert_eq!(record.result, OperationResult::RefillTimeout);
            assert_eq!(record.drain_ran_s, PLAN.drain_s);
            assert_eq!(record.refill_ran_s, 0);
            {
                let water = sim.water();
                assert_eq!(water.drained_l, 0.0);
                assert_eq!(water.added_l, 0.0);
                assert!(water.deficit_l().abs() < 1e-9, "{}", water.describe());
                assert_eq!(water.spilled_l, 0.0);
                assert!(water.balance_error_l().abs() < BALANCE_TOL);
            }

            sim.run_for(STARVED_AFTER).await;
            assert!(!sim.service.status().circulation_starved);
        })
        .await;
}

fn overlap(a: &[(f64, f64)], b: &[(f64, f64)]) -> bool {
    a.iter()
        .any(|&(a_on, a_off)| b.iter().any(|&(b_on, b_off)| a_on < b_off && b_on < a_off))
}

#[tokio::test(start_paused = true)]
async fn a_drain_relay_that_refuses_to_switch_off_keeps_the_makeup_off() {
    LocalSet::new()
        .run_until(async {
            let sim = start_stopping_at(None, "sump_return_level").await;
            sim.service
                .start_water_change(WaterChangePlan {
                    drain_s: 60,
                    settle_s: 30,
                    refill_s: 600,
                })
                .expect("water change started");
            sim.run_until(LONG, "30 s of drain", |status, _| {
                status.operation.drain_ran_s >= 30
            })
            .await;

            sim.relays.refuse_off(Some(PumpId::Drain));
            sim.run_until(LONG, "the refill phase", |status, _| {
                status.operation.state == OperationState::Refilling
            })
            .await;
            sim.run_for(secs(10)).await;

            let status = sim.service.status();
            assert_eq!(status.channels.drain.fault, Some(ChannelFault::OutputError));
            assert!(status.channels.drain.on);
            assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::Interlock));
            assert_eq!(status.operation.refill_ran_s, 0);
            let stuck_l = {
                let water = sim.water();
                println!("drain stuck on: {}", water.describe());
                assert!(water.pump_on(PumpId::Drain));
                assert!(!water.pump_on(PumpId::Makeup));
                assert_eq!(water.added_l, 0.0);
                water.drained_l
            };

            sim.relays.refuse_off(None);
            sim.run_until(LONG, "the water change to finish", |status, _| {
                status.operation.state == OperationState::Idle
            })
            .await;

            let record = sim.service.status().last_water_change.expect("a record");
            let water = sim.water();
            println!(
                "after the drain relay recovered: {record:?} / {}",
                water.describe()
            );
            assert_eq!(record.result, OperationResult::SumpFull);
            assert!(water.drained_l - stuck_l < 0.5);
            assert!(!overlap(
                &water.runs(PumpId::Drain, 0.0),
                &water.runs(PumpId::Makeup, 0.0)
            ));
            assert_eq!(water.spilled_l, 0.0);
            assert!(water.balance_error_l().abs() < BALANCE_TOL);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn starting_a_drain_waits_for_a_confirmed_makeup_off() {
    LocalSet::new()
        .run_until(async {
            let sim = start_stopping_at(None, "sump_return_level").await;
            sim.relays.refuse_off(Some(PumpId::Makeup));
            sim.service.start_water_change(PLAN).unwrap();
            sim.run_for(secs(10)).await;

            let status = sim.service.status();
            assert_eq!(
                status.channels.makeup.fault,
                Some(ChannelFault::OutputError)
            );
            assert_eq!(status.channels.drain.blocked_by, Some(Blocker::Interlock));
            assert_eq!(sim.water().drained_l, 0.0);

            sim.relays.refuse_off(None);
            sim.run_for(secs(3)).await;
            assert!(sim.water().drained_l > 0.0);
        })
        .await;
}
