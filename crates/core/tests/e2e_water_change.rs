#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    Blocker, OperationKind, OperationRecord, OperationResult, OperationState, PumpId, PumpSettings,
    PumpsStatus,
};
use aquarium_core::domain::water::WaterLevelRole;

use sim::{civil, Event, Fault, Layout, Sim, BASE_EPOCH};

const LITRES_TOL: f64 = 0.5;
const LEVEL_TOL_CM: f64 = 0.2;
const SECONDS_TOL: f64 = 1.5;
const BALANCE_TOL: f64 = 1e-6;

const WATER_CHANGE: &str = "/api/water-change";
const TOP_OFF: &str = "/api/pumps/top-off";
const SETTINGS: &str = "/api/pumps/settings";
const RETURN_OVERRIDE: &str = "/api/pumps/return/override";

const LONG: Duration = Duration::from_secs(1800);

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn assert_near(actual: f64, expected: f64, tolerance: f64, what: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{what}: expected {expected:.3} ± {tolerance}, got {actual:.3}"
    );
}

async fn configure(sim: &Sim, stop_sensor: Option<&str>) {
    let stop = stop_sensor.map_or("null".to_owned(), |role| format!("\"{role}\""));
    let layout = sim.water().layout;
    let (code, response) = sim
        .send(
            "PATCH",
            SETTINGS,
            &format!(
                r#"{{"flow_lph":{{"makeup":{},"drain":{}}},"refill_stop_sensor":{stop}}}"#,
                layout.makeup_lph, layout.drain_lph
            ),
        )
        .await;
    assert_eq!(code, 200, "got: {response}");
}

async fn start_water_change(sim: &Sim, drain_s: u32, settle_s: u32, refill_s: u32) {
    let (code, response) = sim
        .send(
            "POST",
            WATER_CHANGE,
            &format!(r#"{{"drain_s":{drain_s},"settle_s":{settle_s},"refill_s":{refill_s}}}"#),
        )
        .await;
    assert_eq!(code, 202, "got: {response}");
}

fn finished(status: &PumpsStatus) -> bool {
    status.operation.state == OperationState::Idle && status.last_water_change.is_some()
}

async fn finish(sim: &Sim) -> OperationRecord {
    sim.run_until(LONG, "the water change to finish", |status, _| {
        finished(status)
    })
    .await;

    sim.status()
        .await
        .last_water_change
        .expect("a finished water change")
}

fn in_state(state: OperationState) -> impl FnMut(&PumpsStatus, &sim::Aquarium) -> bool {
    move |status, _| status.operation.state == state
}

fn assert_balanced(sim: &Sim) {
    let water = sim.water();
    assert!(
        water.balance_error_l().abs() < BALANCE_TOL,
        "water balance off by {} L: {}",
        water.balance_error_l(),
        water.describe()
    );
}

fn assert_record_litres(sim: &Sim, record: &OperationRecord) {
    let water = sim.water();
    let layout = water.layout;
    assert_near(
        f64::from(record.drain_ran_s) * layout.drain_l_per_s(),
        water.drained_l,
        LITRES_TOL,
        "drained litres reported vs model",
    );
    assert_near(
        f64::from(record.refill_ran_s) * layout.makeup_l_per_s(),
        water.added_l,
        LITRES_TOL,
        "refilled litres reported vs model",
    );
}

struct Margins {
    drained_l: f64,
}

async fn return_level_full_water_change(layout: Layout) -> Margins {
    let sim = Sim::start(layout).await;
    configure(&sim, Some("sump_return_level")).await;
    start_water_change(&sim, 300, 30, 750).await;

    sim.run_until(LONG, "refill", in_state(OperationState::Refilling))
        .await;
    sim.run_for(secs(20)).await;
    let live = sim.status().await;
    {
        let water = sim.water();
        assert_near(
            f64::from(live.operation.drained_l.expect("drain flow set")),
            water.drained_l,
            LITRES_TOL,
            "live drained litres",
        );
        assert_near(
            f64::from(live.operation.refilled_l.expect("makeup flow set")),
            water.added_l,
            LITRES_TOL,
            "live refilled litres",
        );
    }

    let record = finish(&sim).await;
    assert_eq!(record.result, OperationResult::SumpFull);
    assert!(!record.resumed);
    assert_record_litres(&sim, &record);
    assert_balanced(&sim);

    let water = sim.water();
    let buffer = layout.buffer_to_sump_pump_l();
    assert_near(
        water.drained_l,
        buffer,
        LITRES_TOL,
        "drain ends at SumpPump",
    );
    assert_near(
        f64::from(record.drain_ran_s),
        buffer / layout.drain_l_per_s(),
        SECONDS_TOL,
        "drain_ran_s",
    );
    assert_near(
        water.added_l,
        water.drained_l,
        LITRES_TOL,
        "added vs drained",
    );
    assert_near(
        water.return_level_cm(),
        layout.return_operating_cm,
        LEVEL_TOL_CM,
        "final return-chamber level",
    );
    assert_near(water.tank_l, layout.tank_weir_l, 1e-9, "tank level");
    assert_eq!(water.spilled_l, 0.0);
    assert_eq!(water.tank_spilled_l, 0.0);
    assert_eq!(water.return_dry_s, 0.0);
    assert_eq!(water.makeup_dry_s, 0.0);

    let dry = water
        .first_after(Event::Sensor(WaterLevelRole::SumpPump, false), 0.0)
        .expect("SumpPump went dry");
    let off = water
        .first_after(Event::Pump(PumpId::Return, false), 0.0)
        .expect("return locked out");
    let wet = water
        .first_after(Event::Sensor(WaterLevelRole::SumpPump, true), dry)
        .expect("SumpPump wet again");
    let on = water
        .first_after(Event::Pump(PumpId::Return, true), off)
        .expect("return restarted");
    assert_near(off, dry, SECONDS_TOL, "return stops when SumpPump goes dry");
    assert!(
        (30.0 - 0.01..=30.0 + SECONDS_TOL).contains(&(on - wet)),
        "return restarted {:.1} s after SumpPump got wet",
        on - wet
    );
    assert!(!water.pump_on(PumpId::Makeup) && !water.pump_on(PumpId::Drain));
    assert!(water.pump_on(PumpId::Return));

    assert_eq!(
        water.count(Event::Sensor(WaterLevelRole::ReservoirReady, false)),
        0
    );
    assert!(water
        .first_after(Event::Sensor(WaterLevelRole::ReservoirFull, false), 0.0)
        .is_some());
    assert!(water.reservoir_l < layout.reservoir_full_l);

    Margins {
        drained_l: water.drained_l,
    }
}

#[tokio::test(start_paused = true)]
async fn return_level_full_water_change_ends_with_sump_full() {
    LocalSet::new()
        .run_until(async {
            let margins = return_level_full_water_change(Layout::REAL).await;
            assert_near(margins.drained_l, 8.77, 0.5, "small chamber buffer");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_wide_return_chamber_doubles_the_margins() {
    LocalSet::new()
        .run_until(async {
            let small = Layout::REAL;
            let wide = Layout::REAL.wide_return_chamber();
            let margins = return_level_full_water_change(wide).await;

            assert_near(margins.drained_l, 19.74, 0.5, "wide chamber buffer");
            assert_near(
                wide.buffer_to_sump_pump_l() / small.buffer_to_sump_pump_l(),
                27.0 / 12.0,
                1e-9,
                "buffer ratio",
            );
            assert_near(small.headroom_to_baffle_l(), 2.58, 0.01, "small headroom");
            assert_near(wide.headroom_to_baffle_l(), 5.805, 0.01, "wide headroom");
            assert_near(small.headroom_to_rim_l(), 54.18, 0.01, "small rim headroom");
            assert_near(wide.headroom_to_rim_l(), 57.405, 0.01, "wide rim headroom");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn real_layout_stop_sensor_in_a_baffled_chamber_never_trips() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_biological")).await;
            start_water_change(&sim, 300, 30, 180).await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::Completed);
            assert_eq!(record.refill_ran_s, 180);
            assert_record_litres(&sim, &record);
            assert_balanced(&sim);

            let water = sim.water();
            let overshoot = water.added_l - water.drained_l;
            assert_eq!(
                water.count(Event::Sensor(WaterLevelRole::SumpBiological, true)),
                0
            );
            assert_near(
                water.drained_l,
                layout.buffer_to_sump_pump_l(),
                LITRES_TOL,
                "drained",
            );
            assert_near(overshoot, 21.23, LITRES_TOL, "overshoot");
            assert_near(
                water.return_level_cm(),
                layout.baffle_cm
                    + (overshoot - layout.headroom_to_baffle_l()) / layout.total_area_l_per_cm(),
                LEVEL_TOL_CM,
                "flooded level",
            );
            assert!(water.return_level_cm() > 33.0);
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn real_layout_stop_sensor_trips_only_once_the_whole_sump_floods() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_biological")).await;
            start_water_change(&sim, 300, 30, 750).await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::SumpFull);
            assert_balanced(&sim);

            let water = sim.water();
            let overshoot = water.added_l - water.drained_l;
            assert_near(
                overshoot,
                layout.headroom_to_level_l(layout.sump_biological.height_cm),
                LITRES_TOL,
                "overshoot before the baffled sensor trips",
            );
            assert_near(overshoot, 28.38, LITRES_TOL, "overshoot");
            assert_near(
                water.return_level_cm(),
                layout.sump_biological.height_cm,
                LEVEL_TOL_CM,
                "flooded level",
            );
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn real_layout_without_stop_sensor_spills_on_a_long_refill() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(150.0);
            let sim = Sim::start(layout).await;
            configure(&sim, None).await;
            start_water_change(&sim, 300, 30, 750).await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::Completed);
            assert_record_litres(&sim, &record);
            assert_balanced(&sim);

            let water = sim.water();
            let expected = water.added_l - water.drained_l - layout.headroom_to_rim_l();
            assert_near(water.spilled_l, expected, LITRES_TOL, "spill");
            assert_near(water.spilled_l, 62.05, LITRES_TOL, "spill");
            assert_near(
                water.return_level_cm(),
                layout.sump_height_cm,
                1e-9,
                "sump at rim",
            );
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_stuck_wet_ready_sensor_still_ends_the_refill_on_an_empty_reservoir() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(5.0);
            let sim = Sim::start(layout).await;
            sim.water().faults[WaterLevelRole::ReservoirReady.index()] = Some(Fault::StuckWet);
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 750).await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::ReservoirEmpty);
            assert_balanced(&sim);
            sim.run_for(secs(10)).await;

            let water = sim.water();
            assert!(!water.pump_on(PumpId::Makeup));
            assert_eq!(water.makeup_dry_s, 0.0);
            assert_near(water.reservoir_l, layout.reserve_wet_l, 0.02, "reservoir");
            assert_near(water.added_l, 2.0, 0.02, "added");
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_return_override_pauses_the_refill() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 750).await;

            sim.run_until(LONG, "refill", in_state(OperationState::Refilling))
                .await;
            sim.run_for(secs(5)).await;

            let (code, response) = sim
                .send("PUT", RETURN_OVERRIDE, r#"{"duration_s":600}"#)
                .await;
            assert_eq!(code, 200, "got: {response}");
            sim.run_for(secs(1)).await;

            let (added, refill_ran) = {
                let status = sim.status().await;
                (sim.water().added_l, status.operation.refill_ran_s)
            };
            sim.run_for(secs(60)).await;

            let paused = sim.status().await;
            assert_eq!(paused.operation.state, OperationState::Refilling);
            assert_eq!(paused.operation.refill_ran_s, refill_ran);
            assert_eq!(paused.channels.makeup.blocked_by, Some(Blocker::ReturnOff));
            {
                let water = sim.water();
                assert_eq!(water.added_l, added);
                assert!(!water.pump_on(PumpId::Makeup));
                assert!(!water.pump_on(PumpId::Return));
            }

            let (code, response) = sim.send("DELETE", RETURN_OVERRIDE, "").await;
            assert_eq!(code, 204, "got: {response}");

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::SumpFull);
            assert_balanced(&sim);

            let water = sim.water();
            assert!(water.added_l > added);
            assert_near(
                water.added_l,
                water.drained_l,
                LITRES_TOL,
                "added vs drained",
            );
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_restart_mid_drain_resumes_and_completes() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.wide_return_chamber();
            let mut sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 40, 30, 750).await;

            sim.run_until(LONG, "25 s of drain", |status, _| {
                status.operation.drain_ran_s >= 25
            })
            .await;
            sim.restart(secs(5), false).await;
            sim.run_for(secs(3)).await;

            let waiting = sim.status().await;
            assert_eq!(waiting.operation.state, OperationState::Resuming);
            assert!(!sim.water().pump_on(PumpId::Drain));

            sim.clock.sync();
            sim.run_until(
                secs(5),
                "drain to resume",
                in_state(OperationState::Draining),
            )
            .await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::SumpFull);
            assert!(record.resumed);
            assert_eq!(record.started_at, Some(BASE_EPOCH));
            assert_balanced(&sim);

            let water = sim.water();
            assert!(
                water.drained_l >= 40.0 * layout.drain_l_per_s() - LITRES_TOL,
                "drained {:.2} L",
                water.drained_l
            );
            assert!(
                water.drained_l <= layout.buffer_to_sump_pump_l() + LITRES_TOL,
                "drained {:.2} L",
                water.drained_l
            );
            assert_near(
                water.added_l,
                water.drained_l,
                LITRES_TOL,
                "added vs drained",
            );
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_second_restart_in_one_operation_suspends() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.wide_return_chamber();
            let mut sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 40, 30, 750).await;

            sim.run_until(LONG, "10 s of drain", |status, _| {
                status.operation.drain_ran_s >= 10
            })
            .await;
            sim.restart(secs(2), true).await;
            sim.run_until(
                secs(5),
                "drain to resume",
                in_state(OperationState::Draining),
            )
            .await;
            sim.run_until(LONG, "more drain", |status, _| {
                status.operation.drain_ran_s >= 20
            })
            .await;

            sim.restart(secs(2), true).await;
            sim.run_for(secs(5)).await;

            let status = sim.status().await;
            assert_eq!(status.operation.state, OperationState::Suspended);
            assert_eq!(status.operation.kind, Some(OperationKind::WaterChange));
            {
                let water = sim.water();
                assert!(!water.pump_on(PumpId::Drain));
                assert!(!water.pump_on(PumpId::Makeup));
                assert!(water.pump_on(PumpId::Return));
            }

            let (code, response) = sim.send("DELETE", WATER_CHANGE, "").await;
            assert_eq!(code, 204, "got: {response}");
            let record = sim.status().await.last_water_change.expect("a record");
            assert_eq!(record.result, OperationResult::InterruptedByReboot);
            assert_balanced(&sim);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_cron_window_fires_and_the_gap_skips_the_next() {
    LocalSet::new()
        .run_until(async {
            let start = civil(BASE_EPOCH);
            assert_eq!(
                (
                    start.weekday,
                    start.hour,
                    start.minute,
                    start.day,
                    start.month
                ),
                (0, 9, 58, 22, 11)
            );

            let sim = Sim::start(Layout::REAL).await;
            configure(&sim, Some("sump_return_level")).await;
            let (code, response) = sim
                .send(
                    "PATCH",
                    SETTINGS,
                    r#"{"water_change":[
                        {"cron":"0 10 * * sun","drain_s":300,"settle_s":30,"refill_s":750},
                        {"cron":"0 11 * * sun","drain_s":300,"settle_s":30,"refill_s":750}]}"#,
                )
                .await;
            assert_eq!(code, 200, "got: {response}");

            let fired = sim
                .run_until(secs(200), "the 10:00 window", |status, _| {
                    status.operation.state != OperationState::Idle
                })
                .await;
            assert_near(fired, 120.0, SECONDS_TOL, "window fired at 10:00");

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::SumpFull);
            assert_eq!(record.started_at, Some(BASE_EPOCH + 120));

            sim.run_until(secs(4000), "the 11:00 window", |status, _| {
                status.last_skipped.is_some()
            })
            .await;
            let status = sim.status().await;
            let skipped = status.last_skipped.expect("a skipped window");
            assert_eq!(skipped.kind, OperationKind::WaterChange);
            assert_eq!(skipped.index, 1);
            assert_eq!(skipped.blocked_by, Some(Blocker::RecentWaterChange));
            assert_eq!(skipped.at, Some(BASE_EPOCH + 3720));
            assert_eq!(status.last_water_change, Some(record));
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_balanced(&sim);
            assert_eq!(sim.water().spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_sump_pump_stuck_wet_runs_the_return_dry() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            sim.water().faults[WaterLevelRole::SumpPump.index()] = Some(Fault::StuckWet);
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 600, 30, 60).await;

            sim.run_until(LONG, "settling", in_state(OperationState::Settling))
                .await;
            let (drained, dry_after_drain) = {
                let water = sim.water();
                let expected = layout.buffer_to_return_intake_l()
                    + (layout.tank_weir_l - layout.drain_intake_l);
                assert_near(water.drained_l, expected, LITRES_TOL, "drained");
                assert_near(water.drained_l, 151.35, LITRES_TOL, "drained");
                assert_near(water.tank_l, layout.drain_intake_l, 1e-9, "tank at intake");
                (water.drained_l, water.return_dry_s)
            };
            let empty_after = layout.buffer_to_return_intake_l() / layout.drain_l_per_s();
            assert_near(dry_after_drain, 600.0 - empty_after, 2.0, "return dry-run");

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::Completed);
            assert_eq!(record.drain_ran_s, 600);
            assert_balanced(&sim);

            let water = sim.water();
            assert_eq!(water.drained_l, drained);
            let partial = 1.0 - layout.makeup_lph / layout.return_lph;
            assert_near(
                water.return_dry_s,
                dry_after_drain + 30.0 + 60.0 * partial,
                3.0,
                "return dry-run at the end",
            );
            assert!(water.return_dry_s > 640.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_level_outage_aborts_the_drain_and_keeps_the_return() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 750).await;

            sim.run_until(LONG, "5 s of drain", |status, _| {
                status.operation.drain_ran_s >= 5
            })
            .await;
            let (outage_at, drained) = {
                let mut water = sim.water();
                water.outage = true;
                (water.t_s, water.drained_l)
            };

            let stopped = sim
                .run_until(secs(5), "drain to stop", |_, water| {
                    !water.pump_on(PumpId::Drain)
                })
                .await;
            assert!(
                stopped - outage_at <= 2.1,
                "drain ran {:.1} s",
                stopped - outage_at
            );

            sim.run_for(secs(5)).await;
            sim.water().outage = false;
            sim.run_for(secs(5)).await;

            let status = sim.status().await;
            let record = status.last_water_change.expect("a record");
            assert_eq!(record.result, OperationResult::Aborted);
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_balanced(&sim);

            let water = sim.water();
            assert!(water.drained_l - drained <= 2.1 * layout.drain_l_per_s());
            assert!(water.pump_on(PumpId::Return));
            assert_eq!(water.count(Event::Pump(PumpId::Return, false)), 0);
            assert_eq!(water.return_dry_s, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn real_layout_faulty_or_stuck_dry_reserve_sensor_refuses_a_top_off() {
    LocalSet::new()
        .run_until(async {
            let sim = Sim::start(Layout::REAL).await;
            sim.water().faults[WaterLevelRole::SumpReserve.index()] = Some(Fault::Faulty);
            configure(&sim, Some("sump_biological")).await;
            sim.run_for(secs(1)).await;

            let top_off = r#"{"duration_s":60}"#;
            let (code, response) = sim.send("POST", TOP_OFF, top_off).await;
            assert_eq!(code, 423, "got: {response}");
            assert!(
                response.contains("the makeup reservoir is empty"),
                "got: {response}"
            );

            sim.water().faults[WaterLevelRole::SumpReserve.index()] = Some(Fault::StuckDry);
            sim.run_for(secs(1)).await;
            let (code, response) = sim.send("POST", TOP_OFF, top_off).await;
            assert_eq!(code, 423, "got: {response}");

            sim.water().faults[WaterLevelRole::SumpReserve.index()] = None;
            sim.run_for(secs(1)).await;
            let (code, response) = sim.send("POST", TOP_OFF, top_off).await;
            assert_eq!(code, 202, "got: {response}");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_blocked_weir_ends_the_refill_with_overflow() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 750).await;

            sim.run_until(LONG, "refill", in_state(OperationState::Refilling))
                .await;
            sim.water().weir_blocked = true;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::Overflow);
            assert_balanced(&sim);
            sim.run_for(secs(10)).await;

            let water = sim.water();
            assert!(water
                .first_after(Event::Sensor(WaterLevelRole::MainTank, true), 0.0)
                .is_some());
            assert!(!water.pump_on(PumpId::Makeup));
            assert!(water.tank_l > layout.tank_weir_l + layout.main_tank_margin_l);
            assert_eq!(water.spilled_l, 0.0);
            assert_eq!(water.tank_spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_default_sensor_tops_off_while_the_reservoir_is_too_low_for_a_change() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(30.0);
            let sim = Sim::start(layout).await;
            let settings: PumpSettings = sim.fetch(SETTINGS).await;
            assert_eq!(
                settings.refill_stop_sensor,
                Some(WaterLevelRole::SumpReturnLevel)
            );

            sim.water().evaporate(2.0);
            sim.run_for(secs(1)).await;

            let (code, response) = sim
                .send(
                    "POST",
                    WATER_CHANGE,
                    r#"{"drain_s":300,"settle_s":30,"refill_s":750}"#,
                )
                .await;
            assert_eq!(code, 423, "got: {response}");
            assert!(
                response.contains("the makeup reservoir does not hold enough for a water change"),
                "got: {response}"
            );

            let (code, response) = sim.send("POST", TOP_OFF, r#"{"duration_s":120}"#).await;
            assert_eq!(code, 202, "got: {response}");

            sim.run_until(LONG, "the top-off to finish", |status, _| {
                status.operation.state == OperationState::Idle && status.last_top_off.is_some()
            })
            .await;
            let status = sim.status().await;
            let record = status.last_top_off.expect("a finished top-off");
            assert_eq!(record.result, OperationResult::SumpFull);
            assert_eq!(status.last_water_change, None);
            assert_balanced(&sim);

            let water = sim.water();
            assert_near(water.added_l, 2.0, LITRES_TOL, "evaporation replaced");
            assert_near(
                water.return_level_cm(),
                layout.return_operating_cm,
                LEVEL_TOL_CM,
                "back at the operating level",
            );
            assert_eq!(water.drained_l, 0.0);
            assert_eq!(water.spilled_l, 0.0);
            assert!(!water.pump_on(PumpId::Makeup));
            assert!(water.pump_on(PumpId::Return));
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_scheduled_water_change_is_skipped_until_the_reservoir_is_ready() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(Layout::REAL.reservoir_ready_l - 1.0);
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            let (code, response) = sim
                .send(
                    "PATCH",
                    SETTINGS,
                    r#"{"water_change":[
                        {"cron":"0 10 * * sun","drain_s":300,"settle_s":30,"refill_s":750}]}"#,
                )
                .await;
            assert_eq!(code, 200, "got: {response}");

            sim.run_until(secs(200), "the 10:00 window", |status, _| {
                status.last_skipped.is_some()
            })
            .await;
            let status = sim.status().await;
            let skipped = status.last_skipped.expect("a skipped window");
            assert_eq!(skipped.kind, OperationKind::WaterChange);
            assert_eq!(skipped.blocked_by, Some(Blocker::ReservoirLow));
            assert_eq!(skipped.at, Some(BASE_EPOCH + 120));
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_eq!(status.last_water_change, None);

            let water = sim.water();
            assert_eq!(water.drained_l, 0.0);
            assert_eq!(water.added_l, 0.0);
            assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 0);
        })
        .await;
}
