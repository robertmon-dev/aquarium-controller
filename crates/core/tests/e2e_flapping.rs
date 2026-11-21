#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    Blocker, OperationResult, OperationState, PumpId, PumpSettingsPatch, WaterChangePlan,
};
use aquarium_core::domain::water::WaterLevelRole;
use aquarium_core::ports::inbound::{PumpControl, StartError};

use sim::{Event, Fault, Layout, Options, Sensors, Sim, FIRMWARE_FILTER, STEP};

const RESERVOIR_L: f64 = 150.0;
const RIPPLE_CM: f64 = 0.3;
const WAVE_CM: f64 = 1.0;
const TOP_OFF_DEFICIT_L: f64 = 15.0;
const EVAPORATION_L_PER_DAY: f64 = 10.0;
const REST_S: f64 = 3.0 * 3600.0;
const SLACK_L: f64 = 0.1;
const SEED: u64 = 0xF1A9;
const RESTART_DELAY_S: f64 = 30.0;
const MIN_MAKEUP_BURST_S: f64 = 5.0;
const BOOT_SETTLE_S: f64 = 32.0;
const LITRES_TOL: f64 = 0.6;
const BALANCE_TOL: f64 = 1e-6;
const EPS_S: f64 = 1e-6;
const PLAN: WaterChangePlan = WaterChangePlan {
    drain_s: 300,
    settle_s: 30,
    refill_s: 900,
};

const LONG: Duration = Duration::from_secs(1800);

fn window_s() -> f64 {
    f64::from(FIRMWARE_FILTER.flap_window_samples) * STEP.as_secs_f64()
}

fn debounce_s() -> f64 {
    f64::from(FIRMWARE_FILTER.debounce_samples) * STEP.as_secs_f64()
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn filtered() -> Options {
    Options {
        sensors: Sensors::Filtered(FIRMWARE_FILTER),
        ..Options::DEFAULT
    }
}

async fn start(wave_cm: f64) -> Sim {
    let sim = Sim::start_with(Layout::REAL.with_reservoir(RESERVOIR_L), filtered()).await;
    {
        let mut water = sim.water();
        water.seed(SEED);
        water.wave_cm = wave_cm;
    }
    let patch: PumpSettingsPatch = serde_json::from_str(
        r#"{"flow_lph":{"makeup":600,"drain":1500},"refill_stop_sensor":"sump_return_level"}"#,
    )
    .expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");
    sim
}

async fn settle_boot(sim: &Sim) {
    sim.run_until(secs(60), "the boot to settle", |_, water| {
        water.t_s >= BOOT_SETTLE_S && water.pump_on(PumpId::Return)
    })
    .await;
}

fn min_restart_gap_s(water: &sim::Aquarium, after_s: f64) -> Option<f64> {
    water
        .runs(PumpId::Return, 0.0)
        .windows(2)
        .filter(|pair| pair[1].0 >= after_s)
        .map(|pair| pair[1].0 - pair[0].1)
        .min_by(f64::total_cmp)
}

#[tokio::test(start_paused = true)]
async fn filtered_sensors_keep_the_return_running_through_every_boot() {
    LocalSet::new()
        .run_until(async {
            let mut sim = start(0.0).await;
            let available = sim
                .run_until(secs(5), "the first level snapshot", |status, _| {
                    status.channels.makeup.blocked_by != Some(Blocker::LevelsUnavailable)
                })
                .await;
            assert!(
                available <= debounce_s() + STEP.as_secs_f64(),
                "levels published {available:.1} s after boot"
            );
            sim.run_for(secs(BOOT_SETTLE_S as u64)).await;

            sim.restart(secs(2), true).await;
            let booted = sim.water().t_s;
            let available = sim
                .run_until(secs(5), "the first level snapshot", |status, _| {
                    status.channels.makeup.blocked_by != Some(Blocker::LevelsUnavailable)
                })
                .await;
            assert!(
                available - booted <= debounce_s() + STEP.as_secs_f64(),
                "levels published {:.1} s after the reboot",
                available - booted
            );
            sim.run_for(secs(BOOT_SETTLE_S as u64)).await;

            let status = sim.service.status();
            assert_eq!(status.channels.return_pump.blocked_by, None);
            assert!(!status.circulation_starved);
            let water = sim.water();
            assert_eq!(water.count(Event::Pump(PumpId::Return, false)), 0);
            assert_eq!(water.count(Event::Pump(PumpId::Uv, false)), 0);
            assert!(water.pump_on(PumpId::Return) && water.pump_on(PumpId::Uv));
            assert_eq!(water.tank_l, water.layout.tank_weir_l);
            assert_eq!(water.return_dry_s, 0.0);
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

struct Waves {
    restarts: usize,
    min_gap_s: f64,
    bursts: usize,
    shortest_burst_s: f64,
    faulty_during: Vec<WaterLevelRole>,
    stop_flagged_at_rest_s: f64,
}

fn stop_overshoot_l(layout: &Layout, wave_cm: f64) -> f64 {
    wave_cm * layout.return_area_l_per_cm() + layout.makeup_l_per_s() * debounce_s()
}

async fn water_change_in_waves(wave_cm: f64) -> Waves {
    let sim = start(wave_cm).await;
    settle_boot(&sim).await;
    let started = sim.water().t_s;
    sim.service
        .start_water_change(PLAN)
        .expect("water change started");

    let carried = roles(sim.published().sensors.map(|reading| reading.faulty));
    let mut faulty_during = [false; WaterLevelRole::COUNT];
    sim.run_until(LONG, "the water change to finish", |status, water| {
        if water.t_s - started > 2.0 * window_s() {
            for reading in sim.published().sensors {
                faulty_during[reading.role.index()] |= reading.faulty;
            }
        }
        status.operation.state == OperationState::Idle && status.last_water_change.is_some()
    })
    .await;
    let record = sim.service.status().last_water_change.expect("a record");
    let finished = sim.water().t_s;
    let deficit_l = sim.water().deficit_l();

    let mut stop_flagged_at_rest_s = 0.0;
    while sim.water().t_s - finished < REST_S {
        sim.water().evaporate(EVAPORATION_L_PER_DAY / 86_400.0);
        sim.run_for(secs(1)).await;
        if sim.published().get(WaterLevelRole::SumpReturnLevel).faulty {
            stop_flagged_at_rest_s += 1.0;
            assert_eq!(
                sim.service.start_top_off(60),
                Err(StartError::Blocked(Blocker::SumpFull))
            );
        }
    }

    let water = sim.water();
    let bursts = water.runs(PumpId::Makeup, started);
    let waves = Waves {
        restarts: water.count_after(Event::Pump(PumpId::Return, true), started),
        min_gap_s: min_restart_gap_s(&water, started).expect("return restarts"),
        bursts: bursts.len(),
        shortest_burst_s: bursts
            .iter()
            .map(|(from, to)| to - from)
            .min_by(f64::total_cmp)
            .expect("makeup ran"),
        faulty_during: roles(faulty_during),
        stop_flagged_at_rest_s,
    };
    println!(
        "waves ±{wave_cm} cm: {record:?}, faulty at start {carried:?}, {} return starts, min restart gap {:.1} s, \
         {} makeup bursts, shortest {:.1} s, deficit {deficit_l:.2} L, faulty during {:?}, \
         stop sensor faulty for {:.0} s of {REST_S} s at rest",
        waves.restarts,
        waves.min_gap_s,
        waves.bursts,
        waves.shortest_burst_s,
        waves.faulty_during,
        waves.stop_flagged_at_rest_s,
    );

    assert_eq!(record.result, OperationResult::SumpFull);
    assert!(
        waves.min_gap_s >= RESTART_DELAY_S - EPS_S,
        "return restarted after {:.1} s",
        waves.min_gap_s
    );
    assert!(
        waves.shortest_burst_s >= MIN_MAKEUP_BURST_S,
        "a makeup burst lasted {:.1} s",
        waves.shortest_burst_s
    );
    assert!(
        deficit_l < LITRES_TOL && -deficit_l < stop_overshoot_l(&water.layout, wave_cm) + SLACK_L,
        "deficit {deficit_l:.2} L"
    );
    assert_eq!(water.spilled_l, 0.0);
    assert_eq!(water.return_dry_s, 0.0);
    assert_eq!(water.makeup_dry_s, 0.0);
    assert!(water.balance_error_l().abs() < BALANCE_TOL);
    waves
}

#[tokio::test(start_paused = true)]
async fn ripple_through_the_firmware_filter_keeps_restarts_and_bursts_sane() {
    LocalSet::new()
        .run_until(async {
            let waves = water_change_in_waves(RIPPLE_CM).await;
            assert!(waves.faulty_during.is_empty());
            assert!(waves.stop_flagged_at_rest_s > 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn waves_through_the_firmware_filter_flag_the_evaporating_stop_sensor() {
    LocalSet::new()
        .run_until(async {
            let waves = water_change_in_waves(WAVE_CM).await;
            assert_eq!(waves.faulty_during, [WaterLevelRole::SumpPump]);
            assert!(waves.stop_flagged_at_rest_s > 0.0);
        })
        .await;
}

fn roles(flags: [bool; WaterLevelRole::COUNT]) -> Vec<WaterLevelRole> {
    WaterLevelRole::ALL
        .into_iter()
        .filter(|role| flags[role.index()])
        .collect()
}

async fn flap_until_faulty(sim: &Sim, role: WaterLevelRole) -> f64 {
    let from = sim.water().t_s;
    sim.water().faults[role.index()] = Some(Fault::Flapping);
    let flagged = sim
        .run_until(secs(120), "the filter to flag the sensor", |_, _| {
            sim.published().get(role).faulty
        })
        .await;
    let latency = flagged - from;
    assert!(
        latency <= 2.0 * window_s() + STEP.as_secs_f64(),
        "{role:?} flagged {latency:.1} s after it began flapping"
    );
    latency
}

async fn calm_until_healthy(sim: &Sim, role: WaterLevelRole) -> f64 {
    let from = sim.water().t_s;
    sim.water().faults[role.index()] = None;
    let cleared = sim
        .run_until(secs(120), "the filter to clear the flag", |_, _| {
            !sim.published().get(role).faulty
        })
        .await;
    let latency = cleared - from;
    assert!(
        latency <= 2.0 * window_s() + STEP.as_secs_f64(),
        "{role:?} cleared {latency:.1} s after it calmed"
    );
    latency
}

#[tokio::test(start_paused = true)]
async fn flapping_sump_pump_sensor_is_flagged_faulty_and_stops_the_return() {
    LocalSet::new()
        .run_until(async {
            let sim = start(0.0).await;
            settle_boot(&sim).await;

            let flagged = flap_until_faulty(&sim, WaterLevelRole::SumpPump).await;
            sim.run_for(secs(2)).await;
            let status = sim.service.status();
            assert_eq!(
                status.channels.return_pump.blocked_by,
                Some(Blocker::SumpLow)
            );
            assert_eq!(status.channels.uv.blocked_by, Some(Blocker::ReturnOff));
            {
                let water = sim.water();
                assert!(!water.pump_on(PumpId::Return) && !water.pump_on(PumpId::Uv));
            }

            let cleared = calm_until_healthy(&sim, WaterLevelRole::SumpPump).await;
            let healthy_at = sim.water().t_s;
            let on = sim
                .run_until(secs(60), "the return to restart", |_, water| {
                    water.pump_on(PumpId::Return)
                })
                .await;
            assert!(
                on - healthy_at >= RESTART_DELAY_S - EPS_S,
                "return restarted {:.1} s after the flag cleared",
                on - healthy_at
            );
            println!(
                "sump_pump flapping: flagged after {flagged:.1} s, cleared after {cleared:.1} s"
            );

            let water = sim.water();
            assert_eq!(water.return_dry_s, 0.0);
            assert_eq!(water.spilled_l, 0.0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn flapping_level_sensors_block_the_makeup_as_dangerous() {
    LocalSet::new()
        .run_until(async {
            for (role, blocker) in [
                (WaterLevelRole::SumpReturnLevel, Blocker::SumpFull),
                (WaterLevelRole::SumpReserve, Blocker::ReservoirEmpty),
                (WaterLevelRole::MainTank, Blocker::TankOverflow),
            ] {
                let sim = start(0.0).await;
                settle_boot(&sim).await;
                sim.water().evaporate_tank(TOP_OFF_DEFICIT_L);
                sim.run_for(secs(10)).await;
                sim.service.start_top_off(600).expect("top-off started");
                sim.run_for(secs(5)).await;
                assert!(sim.water().pump_on(PumpId::Makeup), "{role:?}");

                flap_until_faulty(&sim, role).await;
                sim.run_for(secs(2)).await;
                let status = sim.service.status();
                assert_eq!(status.operation.state, OperationState::Idle, "{role:?}");
                let expected = match blocker {
                    Blocker::SumpFull => OperationResult::SumpFull,
                    Blocker::ReservoirEmpty => OperationResult::ReservoirEmpty,
                    _ => OperationResult::Overflow,
                };
                assert_eq!(
                    status.last_top_off.map(|record| record.result),
                    Some(expected),
                    "{role:?}"
                );
                assert_eq!(status.channels.makeup.blocked_by, Some(blocker), "{role:?}");
                assert_eq!(
                    sim.service.start_top_off(60),
                    Err(StartError::Blocked(blocker)),
                    "{role:?}"
                );
                let water = sim.water();
                assert!(!water.pump_on(PumpId::Makeup), "{role:?}");
                assert_eq!(water.spilled_l, 0.0);
                assert_eq!(water.makeup_dry_s, 0.0);
            }
        })
        .await;
}
