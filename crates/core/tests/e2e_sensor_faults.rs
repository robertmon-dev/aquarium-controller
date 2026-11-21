#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{Blocker, OperationResult, PumpId, PumpSettingsPatch};
use aquarium_core::domain::water::WaterLevelRole;
use aquarium_core::ports::inbound::PumpControl;
use aquarium_core::ports::outbound::PumpStatusSource;

use sim::{Event, Fault, Layout, Options, Sensors, Sim, FIRMWARE_FILTER};

const BALANCE_TOL: f64 = 1e-6;
const WATER_CHANGE: &str = "/api/water-change";
const TOP_OFF: &str = "/api/pumps/top-off";
const PLAN: &str = r#"{"drain_s":112,"settle_s":30,"refill_s":600}"#;
const LONG: Duration = Duration::from_secs(1800);
const STARVED_AFTER: Duration = Duration::from_secs(660);

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn filtered() -> Options {
    Options {
        sensors: Sensors::Filtered(FIRMWARE_FILTER),
        ..Options::DEFAULT
    }
}

async fn start(layout: Layout) -> Sim {
    let sim = Sim::start_with(layout, filtered()).await;
    let patch: PumpSettingsPatch = serde_json::from_str(
        r#"{"flow_lph":{"makeup":600,"drain":1500},"refill_stop_sensor":"sump_return_level"}"#,
    )
    .expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");
    sim.run_for(secs(5)).await;
    sim
}

fn faulty(sim: &Sim) -> Vec<WaterLevelRole> {
    sim.published()
        .sensors
        .iter()
        .filter(|reading| reading.faulty)
        .map(|reading| reading.role)
        .collect()
}

fn assert_balanced(sim: &Sim) {
    let water = sim.water();
    assert!(
        water.balance_error_l().abs() < BALANCE_TOL,
        "balance off by {}",
        water.balance_error_l()
    );
}

#[tokio::test(start_paused = true)]
async fn a_stuck_wet_ready_sensor_over_an_empty_reservoir_refuses_the_water_change() {
    LocalSet::new()
        .run_until(async {
            let sim = start(Layout::REAL.with_reservoir(2.0)).await;
            sim.water().faults[WaterLevelRole::ReservoirReady.index()] = Some(Fault::StuckWet);
            sim.run_for(secs(2)).await;

            assert_eq!(
                faulty(&sim),
                [WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]
            );
            let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("does not hold enough"), "got: {response}");
            let (code, response) = sim.send("POST", TOP_OFF, r#"{"duration_s":60}"#).await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("reservoir is empty"), "got: {response}");

            sim.run_for(secs(5)).await;
            let status = sim.status().await;
            assert_eq!(
                status.channels.makeup.blocked_by,
                Some(Blocker::ReservoirEmpty)
            );
            assert!(status.channels.return_pump.on);
            let water = sim.water();
            assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 0);
            assert_eq!(water.drained_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_stuck_dry_reserve_sensor_under_a_ready_reservoir_refuses_the_water_change() {
    LocalSet::new()
        .run_until(async {
            let sim = start(Layout::REAL.with_reservoir(150.0)).await;
            sim.water().faults[WaterLevelRole::SumpReserve.index()] = Some(Fault::StuckDry);
            sim.run_for(secs(2)).await;

            assert_eq!(
                faulty(&sim),
                [WaterLevelRole::SumpReserve, WaterLevelRole::ReservoirReady]
            );
            let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("does not hold enough"), "got: {response}");

            sim.water().faults[WaterLevelRole::SumpReserve.index()] = None;
            sim.run_for(secs(2)).await;
            assert!(faulty(&sim).is_empty());
            let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
            assert_eq!(code, 202, "got: {response}");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_stuck_wet_return_level_over_a_dry_sump_pump_sensor_is_flagged_faulty() {
    LocalSet::new()
        .run_until(async {
            let sim = start(Layout::REAL.with_reservoir(150.0)).await;
            sim.water().faults[WaterLevelRole::SumpReturnLevel.index()] = Some(Fault::StuckWet);
            sim.run_for(secs(2)).await;
            assert!(faulty(&sim).is_empty());

            {
                let mut water = sim.water();
                let buffer_l = water.layout.buffer_to_sump_pump_l();
                water.weir_blocked = true;
                water.evaporate(buffer_l + 1.0);
            }
            sim.run_for(secs(2)).await;

            assert_eq!(
                faulty(&sim),
                [WaterLevelRole::SumpPump, WaterLevelRole::SumpReturnLevel]
            );
            let status = sim.status().await;
            assert_eq!(
                status.channels.return_pump.blocked_by,
                Some(Blocker::SumpLow)
            );
            assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpFull));
            let (code, response) = sim.send("POST", TOP_OFF, r#"{"duration_s":60}"#).await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("the sump is full"), "got: {response}");
            assert_eq!(sim.water().return_dry_s, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unplugged_harness_is_flagged_and_the_return_keeps_circulating() {
    LocalSet::new()
        .run_until(async {
            let sim = start(Layout::REAL.with_reservoir(150.0)).await;
            sim.water().faults = [Some(Fault::StuckDry); WaterLevelRole::COUNT];
            sim.run_for(secs(3)).await;

            assert!(faulty(&sim).is_empty(), "all-dry is self-consistent");
            let status = sim.status().await;
            assert!(status.level_sensors_suspect);
            assert!(!status.return_held_after_drain);
            assert!(sim.service.pump_telemetry().level_sensors_suspect);
            assert!(status.channels.return_pump.on && status.channels.uv.on);
            assert_eq!(status.channels.return_pump.blocked_by, None);
            assert_eq!(
                status.channels.makeup.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );
            assert_eq!(
                status.channels.drain.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );

            for (path, plan) in [(WATER_CHANGE, PLAN), (TOP_OFF, r#"{"duration_s":60}"#)] {
                let (code, response) = sim.send("POST", path, plan).await;
                assert_eq!(code, 423, "got: {response}");
                assert!(response.contains("unavailable"), "got: {response}");
            }

            sim.run_for(STARVED_AFTER).await;
            let status = sim.status().await;
            assert!(status.level_sensors_suspect);
            assert!(!status.circulation_starved);
            {
                let water = sim.water();
                println!("unplugged: {}", water.describe());
                assert_eq!(water.count(Event::Pump(PumpId::Return, false)), 0);
                assert_eq!(water.return_dry_s, 0.0);
                assert_eq!(water.spilled_l, 0.0);
                assert_eq!(water.tank_l, water.layout.tank_weir_l);
                assert_eq!(water.drained_l + water.added_l, 0.0);
            }
            assert_balanced(&sim);

            sim.water().faults = [None; WaterLevelRole::COUNT];
            sim.run_for(secs(3)).await;
            let status = sim.status().await;
            assert!(!status.level_sensors_suspect);
            assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpFull));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unplugged_harness_mid_drain_aborts_and_holds_the_return_off() {
    LocalSet::new()
        .run_until(async {
            for unplug_after_s in [60, 100] {
                let sim = start(Layout::REAL.with_reservoir(150.0)).await;
                let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
                assert_eq!(code, 202, "got: {response}");
                sim.run_until(LONG, "the drain", |status, _| {
                    status.operation.drain_ran_s >= unplug_after_s
                })
                .await;

                sim.water().faults = [Some(Fault::StuckDry); WaterLevelRole::COUNT];
                sim.run_for(secs(600)).await;

                let status = sim.status().await;
                assert!(status.level_sensors_suspect);
                assert!(status.return_held_after_drain);
                assert_eq!(
                    status.channels.return_pump.blocked_by,
                    Some(Blocker::LevelsUnavailableAfterDrain)
                );
                assert_eq!(
                    status.last_water_change.map(|record| record.result),
                    Some(OperationResult::Aborted)
                );
                {
                    let water = sim.water();
                    println!(
                        "unplugged after {unplug_after_s} s of drain: {}",
                        water.describe()
                    );
                    assert!(!water.pump_on(PumpId::Return));
                    assert!(!water.pump_on(PumpId::Uv));
                    assert!(!water.pump_on(PumpId::Drain));
                    assert_eq!(water.return_dry_s, 0.0, "{}", water.describe());
                    assert_eq!(water.spilled_l, 0.0);
                }
                assert_balanced(&sim);

                sim.water().faults = [None; WaterLevelRole::COUNT];
                sim.run_for(secs(3)).await;
                let status = sim.status().await;
                assert!(!status.level_sensors_suspect);
                assert!(!status.return_held_after_drain);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_shorted_harness_is_flagged_and_refuses_the_water_change() {
    LocalSet::new()
        .run_until(async {
            let sim = start(Layout::REAL.with_reservoir(150.0)).await;
            sim.water().faults = [Some(Fault::StuckWet); WaterLevelRole::COUNT];
            sim.run_for(secs(3)).await;

            assert!(faulty(&sim).is_empty(), "all-wet is self-consistent");
            let status = sim.status().await;
            assert!(status.level_sensors_suspect);
            assert!(status.channels.return_pump.on && status.channels.uv.on);
            assert_eq!(
                status.channels.makeup.blocked_by,
                Some(Blocker::LevelsUnavailable)
            );
            for (path, plan) in [(WATER_CHANGE, PLAN), (TOP_OFF, r#"{"duration_s":60}"#)] {
                let (code, response) = sim.send("POST", path, plan).await;
                assert_eq!(code, 423, "got: {response}");
            }

            sim.run_for(secs(120)).await;
            let status = sim.status().await;
            assert!(status.level_sensors_suspect);
            assert_eq!(status.last_water_change, None);
            {
                let water = sim.water();
                println!("all wet: {}", water.describe());
                assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 0);
                assert_eq!(water.drained_l, 0.0);
                assert_eq!(water.return_dry_s, 0.0);
                assert!(water.deficit_l().abs() < 1e-9);
                assert!(water.pump_on(PumpId::Return));
            }
            assert_balanced(&sim);

            sim.water().faults[WaterLevelRole::MainTank.index()] = None;
            sim.run_for(secs(3)).await;
            let status = sim.status().await;
            assert!(!status.level_sensors_suspect);
            let (code, response) = sim.send("POST", WATER_CHANGE, PLAN).await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("flood sensor"), "got: {response}");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_wet_flood_sensor_ends_a_top_off_and_refuses_the_next() {
    LocalSet::new()
        .run_until(async {
            let sim = start(Layout::REAL.with_reservoir(150.0)).await;
            sim.water().evaporate(5.0);
            sim.run_for(secs(2)).await;
            let (code, response) = sim.send("POST", TOP_OFF, r#"{"duration_s":120}"#).await;
            assert_eq!(code, 202, "got: {response}");
            sim.run_for(secs(5)).await;

            sim.water().faults[WaterLevelRole::SumpBiological.index()] = Some(Fault::StuckWet);
            sim.run_until(secs(5), "the top-off to end", |status, _| {
                status.last_top_off.is_some()
            })
            .await;
            let status = sim.status().await;
            assert!(!status.level_sensors_suspect);
            assert_eq!(
                status.last_top_off.map(|record| record.result),
                Some(OperationResult::Overflow)
            );
            assert_eq!(status.channels.makeup.blocked_by, Some(Blocker::SumpHigh));
            for (path, plan) in [(WATER_CHANGE, PLAN), (TOP_OFF, r#"{"duration_s":60}"#)] {
                let (code, response) = sim.send("POST", path, plan).await;
                assert_eq!(code, 423, "got: {response}");
                assert!(response.contains("flood sensor"), "got: {response}");
            }
            assert!(sim.water().pump_on(PumpId::Return));
            assert_eq!(sim.water().spilled_l, 0.0);
        })
        .await;
}
