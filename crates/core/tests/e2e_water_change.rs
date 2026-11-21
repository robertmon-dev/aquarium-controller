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

struct Exchange {
    record: OperationRecord,
    drained_l: f64,
    added_l: f64,
    tank_after_drain_l: f64,
    return_level_after_settle_cm: f64,
    return_starts: usize,
    return_dry_s: f64,
    makeup_dry_s: f64,
    drain_dry_s: f64,
}

fn expected_drain_l(layout: &Layout, drain_s: u32) -> f64 {
    (f64::from(drain_s) * layout.drain_l_per_s())
        .min(layout.tank_weir_l - layout.weir_skim_l - layout.drain_intake_l)
}

fn assert_return_cycles_on_the_sump_pump_sensor(water: &sim::Aquarium, after_s: f64) {
    for &(at, event) in &water.log {
        if at < after_s {
            continue;
        }

        if event == Event::Sensor(WaterLevelRole::SumpPump, true) {
            let on = water
                .first_after(Event::Pump(PumpId::Return, true), at)
                .expect("return restarted");
            assert!(
                (30.0 - 0.01..=30.0 + SECONDS_TOL).contains(&(on - at)),
                "return restarted {:.1} s after SumpPump got wet at {at:.1} s",
                on - at
            );
        }

        if event == Event::Pump(PumpId::Return, false) {
            let dry = water
                .log
                .iter()
                .rev()
                .find(|(when, logged)| {
                    *when <= at && *logged == Event::Sensor(WaterLevelRole::SumpPump, false)
                })
                .map(|(when, _)| *when)
                .expect("return stopped on a dry SumpPump");
            assert_near(at, dry, SECONDS_TOL, "return stops when SumpPump goes dry");
        }
    }
}

async fn water_change_exchange(
    layout: Layout,
    drain_s: u32,
    expected: OperationResult,
) -> Exchange {
    let sim = Sim::start(layout).await;
    configure(&sim, Some("sump_return_level")).await;
    let started = sim.water().t_s;
    start_water_change(&sim, drain_s, 30, 900).await;

    sim.run_for(secs(2)).await;
    let draining = sim.status().await;
    assert_eq!(draining.operation.state, OperationState::Draining);
    assert_eq!(
        draining.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );
    assert_eq!(draining.channels.uv.blocked_by, Some(Blocker::ReturnOff));
    {
        let water = sim.water();
        assert!(!water.pump_on(PumpId::Return) && !water.pump_on(PumpId::Uv));
        assert!(water.pump_on(PumpId::Drain));
    }

    sim.run_until(LONG, "settling", in_state(OperationState::Settling))
        .await;
    let tank_after_drain_l = sim.water().tank_l;
    sim.run_for(secs(15)).await;
    let settling = sim.status().await;
    assert_eq!(settling.operation.state, OperationState::Settling);
    assert_eq!(
        settling.channels.return_pump.blocked_by,
        Some(Blocker::WaterChange)
    );

    let refill_at = sim
        .run_until(LONG, "refill", in_state(OperationState::Refilling))
        .await;
    let return_level_after_settle_cm = sim.water().return_level_cm();
    let flooded = sim.status().await;
    assert_eq!(flooded.channels.makeup.blocked_by, Some(Blocker::SumpFull));
    assert_eq!(flooded.channels.return_pump.blocked_by, None);
    assert_eq!(flooded.operation.refill_ran_s, 0);
    sim.run_for(secs(20)).await;
    let live = sim.status().await;
    {
        let water = sim.water();
        assert_near(
            f64::from(live.operation.drained_l.expect("drain flow set")),
            water.drained_l + water.drain_dry_s * layout.drain_l_per_s(),
            LITRES_TOL,
            "live drained litres, dry-run included",
        );
        assert_near(
            f64::from(live.operation.refilled_l.expect("makeup flow set")),
            water.added_l,
            LITRES_TOL,
            "live refilled litres",
        );
    }

    let record = finish(&sim).await;
    assert_eq!(record.result, expected);
    assert!(!record.resumed);
    assert_eq!(record.drain_ran_s, drain_s);
    assert_balanced(&sim);
    sim.run_for(secs(5)).await;

    let water = sim.water();
    assert_near(
        f64::from(record.drain_ran_s) * layout.drain_l_per_s(),
        water.drained_l + water.drain_dry_s * layout.drain_l_per_s(),
        LITRES_TOL,
        "drained litres reported vs model, dry-run included",
    );
    assert_near(
        f64::from(record.refill_ran_s) * layout.makeup_l_per_s(),
        water.added_l,
        LITRES_TOL,
        "refilled litres reported vs model",
    );
    let return_on = water
        .first_after(Event::Pump(PumpId::Return, true), started)
        .expect("return restarted");
    assert_near(
        water
            .first_after(Event::Pump(PumpId::Return, false), started)
            .expect("return stopped"),
        started,
        SECONDS_TOL,
        "return stops when the drain starts",
    );
    assert_near(
        return_on,
        refill_at,
        SECONDS_TOL,
        "return restarts on refill",
    );
    assert_return_cycles_on_the_sump_pump_sensor(&water, refill_at);

    assert_near(
        water.drained_l,
        expected_drain_l(&layout, drain_s),
        LITRES_TOL,
        "drained",
    );
    assert_near(
        water.added_l,
        water.drained_l,
        LITRES_TOL,
        "added vs drained",
    );
    assert_near(
        water.tank_l,
        layout.tank_weir_l,
        1e-9,
        "tank back at the weir",
    );
    assert_near(
        water.return_level_cm(),
        layout.return_operating_cm,
        LEVEL_TOL_CM,
        "final return-chamber level",
    );
    assert_near(
        return_level_after_settle_cm,
        layout.return_level_with_l(layout.weir_skim_l),
        LEVEL_TOL_CM,
        "return chamber flooded by the weir skim",
    );
    let stop_dry = water
        .first_after(
            Event::Sensor(WaterLevelRole::SumpReturnLevel, false),
            refill_at,
        )
        .expect("the return pump lowers the return chamber");
    let makeup_on = water
        .first_after(Event::Pump(PumpId::Makeup, true), refill_at)
        .expect("makeup ran");
    assert!(
        makeup_on >= stop_dry && makeup_on - stop_dry <= SECONDS_TOL,
        "makeup started {:.1} s after the stop sensor dried",
        makeup_on - stop_dry
    );
    assert_eq!(water.spilled_l, 0.0);
    assert_eq!(water.tank_spilled_l, 0.0);
    assert!(!water.pump_on(PumpId::Makeup) && !water.pump_on(PumpId::Drain));
    assert!(water.pump_on(PumpId::Return) && water.pump_on(PumpId::Uv));
    assert!(water
        .first_after(Event::Sensor(WaterLevelRole::ReservoirFull, false), 0.0)
        .is_some());

    Exchange {
        record,
        drained_l: water.drained_l,
        added_l: water.added_l,
        tank_after_drain_l,
        return_level_after_settle_cm,
        return_starts: water.count_after(Event::Pump(PumpId::Return, true), refill_at),
        return_dry_s: water.return_dry_s,
        makeup_dry_s: water.makeup_dry_s,
        drain_dry_s: water.drain_dry_s,
    }
}

#[tokio::test(start_paused = true)]
async fn return_level_typical_water_change_exchanges_the_drained_volume() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(150.0);
            let exchange = water_change_exchange(layout, 300, OperationResult::SumpFull).await;

            assert_near(exchange.drained_l, 125.0, LITRES_TOL, "drained");
            assert_near(exchange.added_l, 125.0, LITRES_TOL, "added");
            assert_near(
                exchange.tank_after_drain_l,
                layout.tank_weir_l - layout.weir_skim_l - 125.0,
                LITRES_TOL,
                "tank after the drain",
            );
            assert_near(
                f64::from(exchange.record.refill_ran_s),
                750.0,
                3.0,
                "refill_ran_s",
            );
            assert_near(
                exchange.return_level_after_settle_cm,
                30.08,
                LEVEL_TOL_CM,
                "skim floods the small chamber over the baffle",
            );
            assert!(
                (18..=20).contains(&exchange.return_starts),
                "return starts: {}",
                exchange.return_starts
            );
            assert_eq!(exchange.return_dry_s, 0.0);
            assert_eq!(exchange.makeup_dry_s, 0.0);
            assert_eq!(exchange.drain_dry_s, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_wide_return_chamber_exchanges_the_same_volume() {
    LocalSet::new()
        .run_until(async {
            let small = Layout::REAL;
            let wide = Layout::REAL.wide_return_chamber().with_reservoir(150.0);
            let exchange = water_change_exchange(wide, 300, OperationResult::SumpFull).await;

            assert_near(exchange.drained_l, 125.0, LITRES_TOL, "drained");
            assert_near(exchange.added_l, 125.0, LITRES_TOL, "added");
            assert_near(
                f64::from(exchange.record.refill_ran_s),
                750.0,
                3.0,
                "refill_ran_s",
            );
            assert_near(
                exchange.return_level_after_settle_cm,
                27.58,
                LEVEL_TOL_CM,
                "skim stays below the baffle in the wide chamber",
            );
            assert!(
                (15..=17).contains(&exchange.return_starts),
                "return starts: {}",
                exchange.return_starts
            );
            assert_eq!(exchange.return_dry_s, 0.0);
            assert_eq!(exchange.makeup_dry_s, 0.0);
            assert_eq!(exchange.drain_dry_s, 0.0);

            assert_near(small.buffer_to_sump_pump_l(), 8.772, 0.01, "small buffer");
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
async fn return_level_long_drain_is_bounded_by_the_drain_intake() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(200.0);
            let exchange = water_change_exchange(layout, 600, OperationResult::Incomplete).await;

            let reachable_l = layout.tank_weir_l - layout.weir_skim_l - layout.drain_intake_l;
            assert_near(exchange.drained_l, reachable_l, LITRES_TOL, "drained");
            assert_near(exchange.drained_l, 137.0, LITRES_TOL, "drained");
            assert_near(
                exchange.tank_after_drain_l,
                layout.drain_intake_l,
                1e-9,
                "tank at the intake",
            );
            assert_near(
                exchange.drain_dry_s,
                600.0 - reachable_l / layout.drain_l_per_s(),
                2.0,
                "drain dry-run",
            );
            assert_near(exchange.drain_dry_s, 271.2, 2.0, "drain dry-run");
            assert_eq!(exchange.record.drain_ran_s, 600);
            assert_near(
                f64::from(exchange.record.drain_ran_s) * layout.drain_l_per_s(),
                250.0,
                LITRES_TOL,
                "drained litres the record implies",
            );
            assert!(
                (19..=21).contains(&exchange.return_starts),
                "return starts: {}",
                exchange.return_starts
            );
            assert_eq!(exchange.return_dry_s, 0.0);
            assert_eq!(exchange.makeup_dry_s, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn real_layout_default_reservoir_cannot_cover_a_typical_drain() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL;
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 900).await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::ReservoirEmpty);
            assert_balanced(&sim);
            sim.run_for(secs(60)).await;

            let water = sim.water();
            let usable_l = layout.reservoir_l - layout.reserve_wet_l;
            assert_near(water.drained_l, 125.0, LITRES_TOL, "drained");
            assert_near(water.added_l, usable_l, 0.05, "added");
            assert_near(water.tank_l, 680.9, 1.0, "tank left short of the weir");
            assert!(water.return_level_cm() < layout.sump_pump.height_cm);
            assert!(!water.pump_on(PumpId::Return));
            assert_eq!(water.makeup_dry_s, 0.0);
            assert_eq!(water.return_dry_s, 0.0);
            assert_eq!(water.spilled_l, 0.0);
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
            start_water_change(&sim, 30, 30, 180).await;

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
                30.0 * layout.drain_l_per_s(),
                LITRES_TOL,
                "drained",
            );
            assert_near(overshoot, 17.5, LITRES_TOL, "overshoot");
            assert_near(
                water.return_level_cm(),
                layout.baffle_cm
                    + (overshoot - layout.headroom_to_baffle_l()) / layout.total_area_l_per_cm(),
                LEVEL_TOL_CM,
                "flooded level",
            );
            assert!(water.return_level_cm() > 32.5);
            assert_near(water.tank_l, layout.tank_weir_l, 1e-9, "tank at the weir");
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
            start_water_change(&sim, 30, 30, 750).await;

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
            start_water_change(&sim, 30, 30, 750).await;

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::Completed);
            assert_record_litres(&sim, &record);
            assert_balanced(&sim);

            let water = sim.water();
            let expected = water.added_l - water.drained_l - layout.headroom_to_rim_l();
            assert_near(water.spilled_l, expected, LITRES_TOL, "spill");
            assert_near(water.spilled_l, 58.32, LITRES_TOL, "spill");
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
            let layout = Layout::REAL.with_reservoir(150.0);
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 900).await;

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
            assert_eq!(paused.channels.return_pump.blocked_by, None);
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
            assert_near(water.tank_l, layout.tank_weir_l, 1e-9, "tank at the weir");
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
            sim.run_for(secs(2)).await;
            let resumed = sim.status().await;
            assert_eq!(
                resumed.channels.return_pump.blocked_by,
                Some(Blocker::WaterChange)
            );
            assert!(!sim.water().pump_on(PumpId::Return));
            assert!(sim.water().pump_on(PumpId::Drain));

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
                water.drained_l <= 50.0 * layout.drain_l_per_s() + LITRES_TOL,
                "drained {:.2} L",
                water.drained_l
            );
            assert_near(water.tank_l, layout.tank_weir_l, 1e-9, "tank at the weir");
            assert_near(
                water.return_level_cm(),
                layout.return_operating_cm,
                LEVEL_TOL_CM,
                "final return-chamber level",
            );
            assert_eq!(water.return_dry_s, 0.0);
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

            let sim = Sim::start(Layout::REAL.with_reservoir(200.0)).await;
            configure(&sim, Some("sump_return_level")).await;
            let (code, response) = sim
                .send(
                    "PATCH",
                    SETTINGS,
                    r#"{"water_change":[
                        {"cron":"0 10 * * sun","drain_s":300,"settle_s":30,"refill_s":900},
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
async fn return_level_sump_pump_stuck_wet_runs_the_return_dry_while_refilling() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(150.0);
            let sim = Sim::start(layout).await;
            sim.water().faults[WaterLevelRole::SumpPump.index()] = Some(Fault::StuckWet);
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 300, 30, 900).await;

            sim.run_until(LONG, "refill", in_state(OperationState::Refilling))
                .await;
            {
                let water = sim.water();
                assert_near(water.drained_l, 125.0, LITRES_TOL, "drained");
                assert_eq!(water.return_dry_s, 0.0);
            }

            let record = finish(&sim).await;
            assert_eq!(record.result, OperationResult::SumpFull);
            assert_balanced(&sim);

            let water = sim.water();
            let r = layout.return_l_per_s();
            let m = layout.makeup_l_per_s();
            let to_intake_s = layout.buffer_to_return_intake_l() / (r - m);
            let starved_s = (water.drained_l - r * to_intake_s) / m;
            let expected_dry_s = starved_s * (1.0 - m / r);
            assert_near(water.return_dry_s, expected_dry_s, 5.0, "return dry-run");
            assert_near(water.return_dry_s, 531.9, 5.0, "return dry-run");
            assert_eq!(water.count(Event::Pump(PumpId::Return, false)), 1);
            assert_near(
                water.added_l,
                water.drained_l,
                LITRES_TOL,
                "added vs drained",
            );
            assert_near(water.tank_l, layout.tank_weir_l, 1e-9, "tank at the weir");
            assert_eq!(water.spilled_l, 0.0);
            assert_eq!(water.makeup_dry_s, 0.0);
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
            assert_eq!(water.count(Event::Pump(PumpId::Return, false)), 1);
            let restarted = water
                .first_after(Event::Pump(PumpId::Return, true), 0.0)
                .expect("return back on");
            assert!(
                restarted - outage_at <= 2.1,
                "return off {:.1} s into the outage",
                restarted - outage_at
            );
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
            start_water_change(&sim, 60, 30, 750).await;

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

#[tokio::test(start_paused = true)]
async fn return_level_stuck_wet_stop_sensor_times_out_the_refill() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(150.0);
            let sim = Sim::start(layout).await;
            sim.water().faults[WaterLevelRole::SumpReturnLevel.index()] = Some(Fault::StuckWet);
            configure(&sim, Some("sump_return_level")).await;
            start_water_change(&sim, 10, 30, 60).await;

            let refill_at = sim
                .run_until(LONG, "refill", in_state(OperationState::Refilling))
                .await;
            sim.run_for(secs(120)).await;
            let stalled = sim.status().await;
            assert_eq!(stalled.operation.state, OperationState::Refilling);
            assert_eq!(stalled.operation.refill_ran_s, 0);
            assert_eq!(stalled.channels.makeup.blocked_by, Some(Blocker::SumpFull));

            let ended_at = sim
                .run_until(LONG, "the refill to time out", |status, _| finished(status))
                .await;
            let budget_s = 360.0;
            assert!(
                (budget_s..=budget_s + SECONDS_TOL).contains(&(ended_at - refill_at)),
                "refill timed out {:.1} s after it began",
                ended_at - refill_at
            );

            let status = sim.status().await;
            let record = status.last_water_change.expect("a finished water change");
            assert_eq!(record.result, OperationResult::RefillTimeout);
            assert_eq!(record.refill_ran_s, 0);
            assert_eq!(record.drain_ran_s, 10);
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_balanced(&sim);

            sim.run_for(secs(120)).await;
            let after = sim.status().await;
            assert_eq!(after.channels.return_pump.blocked_by, None);
            assert_eq!(after.channels.makeup.blocked_by, Some(Blocker::SumpFull));
            let (code, response) = sim.send("POST", TOP_OFF, r#"{"duration_s":60}"#).await;
            assert_eq!(code, 423, "got: {response}");

            let water = sim.water();
            assert_eq!(water.count(Event::Pump(PumpId::Makeup, true)), 0);
            assert_eq!(water.added_l, 0.0);
            assert_near(
                water.drained_l,
                10.0 * layout.drain_l_per_s(),
                LITRES_TOL,
                "drained",
            );
            assert!(water.pump_on(PumpId::Return) && water.pump_on(PumpId::Uv));
            assert_eq!(water.count(Event::Pump(PumpId::Return, false)), 1);
            assert_eq!(water.return_dry_s, 0.0);
            assert_near(water.tank_l, layout.tank_weir_l, 1e-9, "tank at the weir");
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn return_level_drain_beyond_the_ready_reservoir_is_refused() {
    LocalSet::new()
        .run_until(async {
            let layout = Layout::REAL.with_reservoir(150.0);
            let sim = Sim::start(layout).await;
            configure(&sim, Some("sump_return_level")).await;
            let ready_l = layout.reservoir_ready_l - layout.reserve_wet_l;
            let (code, response) = sim
                .send(
                    "PATCH",
                    SETTINGS,
                    &format!(r#"{{"reservoir_ready_l":{ready_l}}}"#),
                )
                .await;
            assert_eq!(code, 200, "got: {response}");
            sim.run_for(secs(1)).await;

            let longest_s = (ready_l * 3600.0 / layout.drain_lph).floor();
            assert_eq!(longest_s, 112.0);

            let (code, response) = sim
                .send(
                    "POST",
                    WATER_CHANGE,
                    r#"{"drain_s":300,"settle_s":30,"refill_s":750}"#,
                )
                .await;
            assert_eq!(code, 400, "got: {response}");
            assert!(
                response.contains("the planned drain exceeds the water the reservoir holds"),
                "got: {response}"
            );

            let window = |drain_s: f64| {
                format!(
                    r#"{{"water_change":[{{"cron":"0 10 * * sun","drain_s":{drain_s},"settle_s":30,"refill_s":750}}]}}"#
                )
            };
            let (code, response) = sim.send("PATCH", SETTINGS, &window(300.0)).await;
            assert_eq!(code, 400, "got: {response}");
            assert!(
                response.contains("a water-change window drains more than reservoir_ready_l holds"),
                "got: {response}"
            );
            let (code, response) = sim.send("PATCH", SETTINGS, &window(longest_s)).await;
            assert_eq!(code, 200, "got: {response}");

            let faster_drain = format!(
                r#"{{"flow_lph":{{"makeup":{},"drain":{}}}}}"#,
                layout.makeup_lph,
                layout.drain_lph + 100.0
            );
            let (code, response) = sim.send("PATCH", SETTINGS, &faster_drain).await;
            assert_eq!(code, 400, "got: {response}");
            let (code, response) = sim
                .send(
                    "PATCH",
                    SETTINGS,
                    &format!(r#"{{"reservoir_ready_l":{}}}"#, ready_l - 1.0),
                )
                .await;
            assert_eq!(code, 400, "got: {response}");

            let settings: PumpSettings = sim.fetch(SETTINGS).await;
            assert_eq!(settings.reservoir_ready_l, Some(47));
            assert_eq!(settings.flow_lph.drain, Some(1500));
            assert_eq!(settings.water_change.len(), 1);
            assert_eq!(settings.water_change[0].drain_s, 112);

            sim.run_for(secs(5)).await;
            let status = sim.status().await;
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_eq!(status.last_water_change, None);
            let water = sim.water();
            assert_eq!(water.drained_l, 0.0);
            assert_eq!(water.added_l, 0.0);
            assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 0);
            assert!(water.pump_on(PumpId::Return));
        })
        .await;
}
