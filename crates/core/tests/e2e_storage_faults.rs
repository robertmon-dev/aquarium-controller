#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    OperationResult, OperationState, PumpId, PumpSettingsPatch, SkipReason, WaterChangePlan,
};
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::PumpRecord;

use sim::{Event, Layout, Options, Sim};

const RESERVOIR_L: f64 = 150.0;
const WATER_CHANGE: &str = "/api/water-change";
const WATER_CHANGE_RESUME: &str = "/api/water-change/resume";
const TOP_OFF: &str = "/api/pumps/top-off";
const PLAN: &str = r#"{"drain_s":60,"settle_s":30,"refill_s":600}"#;
const LONG: Duration = Duration::from_secs(1800);

const SCHEDULED: &str = r#"{
    "flow_lph":{"makeup":600,"drain":1500},
    "water_change":[{"cron":"* * * * *","drain_s":60,"settle_s":30,"refill_s":600}]}"#;

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn checkpoint(record: PumpRecord, _bytes: &[u8]) -> bool {
    record == PumpRecord::Checkpoint
}

async fn start() -> Sim {
    let sim = Sim::start_with(Layout::REAL.with_reservoir(RESERVOIR_L), Options::DEFAULT).await;
    let patch: PumpSettingsPatch = serde_json::from_str(
        r#"{"flow_lph":{"makeup":600,"drain":1500},"refill_stop_sensor":"sump_return_level"}"#,
    )
    .expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");
    sim.run_for(secs(3)).await;
    sim
}

fn assert_switched_on_only_when_recorded(sim: &Sim) {
    for (id, recorded) in sim.relays.switch_ons() {
        if matches!(id, PumpId::Drain | PumpId::Makeup) {
            assert!(recorded, "{id:?} switched on before pump_op was stored");
        }
    }
}

#[tokio::test(start_paused = true)]
async fn a_scheduled_water_change_is_recorded_before_the_drain_switches_on() {
    LocalSet::new()
        .run_until(async {
            let sim = start().await;
            let patch: PumpSettingsPatch = serde_json::from_str(SCHEDULED).expect("json");
            sim.service.apply_settings(patch).expect("settings");

            sim.run_until(LONG, "the scheduled water change", |status, _| {
                status.last_water_change.is_some()
            })
            .await;

            let switch_ons = sim.relays.switch_ons();
            println!("switch-ons with pump_op stored: {switch_ons:?}");
            assert!(switch_ons.contains(&(PumpId::Drain, true)));
            assert_switched_on_only_when_recorded(&sim);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_start_that_cannot_be_recorded_never_switches_a_water_mover_on() {
    LocalSet::new()
        .run_until(async {
            let sim = start().await;
            sim.water().evaporate(4.0);
            sim.store.refuse(Some(checkpoint));

            for (path, plan) in [(WATER_CHANGE, PLAN), (TOP_OFF, r#"{"duration_s":60}"#)] {
                let (code, response) = sim.send("POST", path, plan).await;
                assert_eq!(code, 503, "got: {response}");
                assert!(
                    response.contains("could not be recorded"),
                    "got: {response}"
                );
            }

            let patch: PumpSettingsPatch = serde_json::from_str(SCHEDULED).expect("json");
            sim.service.apply_settings(patch).expect("settings");
            sim.run_for(secs(120)).await;

            let status = sim.status().await;
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_eq!(
                status.last_skipped.map(|skipped| skipped.reason),
                Some(SkipReason::Storage)
            );
            let water = sim.water();
            assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 0);
            assert_eq!(water.count(Event::Pump(PumpId::Makeup, true)), 0);
            assert_eq!(water.drained_l + water.added_l, 0.0);
            assert!(water.pump_on(PumpId::Return));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_resume_that_cannot_be_recorded_never_switches_the_drain_on() {
    LocalSet::new()
        .run_until(async {
            for automatic in [true, false] {
                let mut sim = start().await;
                sim.service
                    .start_water_change(WaterChangePlan {
                        drain_s: 60,
                        settle_s: 30,
                        refill_s: 600,
                    })
                    .expect("idle");
                sim.run_until(LONG, "20 s of drain", |status, _| {
                    status.operation.drain_ran_s >= 20
                })
                .await;

                let outage = if automatic { secs(60) } else { secs(3600) };
                sim.restart(outage, true).await;
                sim.store.refuse(Some(checkpoint));
                let drained_l = sim.water().drained_l;
                sim.run_until(secs(60), "a suspended water change", |status, _| {
                    status.operation.state == OperationState::Suspended
                })
                .await;

                let (code, response) = sim.send("POST", WATER_CHANGE_RESUME, "").await;
                assert_eq!(code, 503, "automatic {automatic}: {response}");
                sim.run_for(secs(30)).await;

                let status = sim.status().await;
                assert_eq!(status.operation.state, OperationState::Suspended);
                let water = sim.water();
                println!(
                    "resume refused (automatic {automatic}): {}",
                    water.describe()
                );
                assert!(!water.pump_on(PumpId::Drain));
                assert_eq!(water.drained_l, drained_l, "automatic {automatic}");
                assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 1);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_cancel_that_cannot_be_recorded_stops_the_drain_and_lands_once_storage_recovers() {
    LocalSet::new()
        .run_until(async {
            let mut sim = start().await;
            let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
            assert_eq!(code, 202, "got: {response}");
            sim.run_until(LONG, "20 s of drain", |status, _| {
                status.operation.drain_ran_s >= 20
            })
            .await;

            sim.store.refuse(Some(checkpoint));
            let (code, response) = sim.send("DELETE", WATER_CHANGE, "").await;
            assert_eq!(code, 503, "got: {response}");
            assert!(response.contains("not recorded"), "got: {response}");
            sim.run_for(secs(1)).await;

            let status = sim.status().await;
            assert!(status.invalidation_pending);
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_eq!(
                status.last_water_change.map(|record| record.result),
                Some(OperationResult::Aborted)
            );
            assert!(!sim.water().pump_on(PumpId::Drain));
            assert!(sim.store.holds_checkpoint());

            assert_eq!(sim.send("DELETE", WATER_CHANGE, "").await.0, 503);
            assert_eq!(sim.send("DELETE", TOP_OFF, "").await.0, 204);

            let refused = sim.store.ops().iter().filter(|op| !op.applied).count();
            sim.run_for(secs(5)).await;
            assert!(sim.store.ops().iter().filter(|op| !op.applied).count() >= refused + 5);

            sim.store.refuse(None);
            sim.run_for(secs(2)).await;
            assert!(!sim.status().await.invalidation_pending);
            assert!(!sim.store.holds_checkpoint());

            let drained_l = sim.water().drained_l;
            sim.power_cut(secs(30), true).await;
            sim.run_for(secs(120)).await;

            let status = sim.status().await;
            assert_eq!(status.operation.state, OperationState::Idle);
            let water = sim.water();
            println!("cancel recorded late: {}", water.describe());
            assert!(!water.pump_on(PumpId::Drain));
            assert_eq!(water.drained_l, drained_l);
            assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 1);
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_cancelled_drain_keeps_its_gap_even_if_history_is_unwritable_at_restart() {
    LocalSet::new()
        .run_until(async {
            for restart_before_recovery in [true, false] {
                let mut sim = start().await;
                assert_eq!(sim.send("POST", WATER_CHANGE, PLAN).await.0, 202);
                sim.run_for(secs(12)).await;
                assert!(sim.water().drained_l > 0.0);
                sim.store.refuse(Some(|_, _| true));
                assert_eq!(sim.send("DELETE", WATER_CHANGE, "").await.0, 503);
                sim.store
                    .refuse(Some(|record, _| record == PumpRecord::LastOperations));
                sim.run_for(secs(2)).await;
                assert!(!sim.status().await.invalidation_pending);
                assert_eq!(sim.send("DELETE", WATER_CHANGE, "").await.0, 204);
                if !restart_before_recovery {
                    sim.store.refuse(None);
                    sim.run_for(secs(2)).await;
                }
                let drained = sim.water().drained_l;
                sim.restart(secs(5), true).await;
                sim.run_for(secs(3)).await;
                assert_eq!(sim.water().drained_l, drained);
                assert_eq!(
                    sim.status().await.last_water_change.unwrap().result,
                    OperationResult::Aborted
                );
                sim.store.refuse(None);
                sim.run_for(secs(2)).await;
                assert_eq!(sim.send("DELETE", WATER_CHANGE, "").await.0, 204);
                let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
                assert_eq!(code, 423, "{response}");
                assert!(response.contains("too recent"), "{response}");
            }
        })
        .await;
}
