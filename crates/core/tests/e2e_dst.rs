#![recursion_limit = "256"]

mod common;
mod sim;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    OperationRecord, OperationResult, OperationState, PumpId, PumpSettingsPatch, SkippedWindow,
};
use aquarium_core::ports::inbound::PumpControl;

use sim::{civil, utc, Event, Layout, Options, Sim, Zone};

const RESERVOIR_L: f64 = 150.0;
const EVAPORATED_L: f64 = 1.0;

const TOP_OFF_AT_0230: &str = r#"{
    "flow_lph":{"makeup":600,"drain":1500},
    "top_off":[{"cron":"30 2 * * *","duration_s":120}]}"#;

const TOP_OFF_AT_0200_AND_0230: &str = r#"{
    "flow_lph":{"makeup":600,"drain":1500},
    "top_off":[{"cron":"0,30 2 * * *","duration_s":120}]}"#;

const WATER_CHANGE_AT_0230: &str = r#"{
    "flow_lph":{"makeup":600,"drain":1500},
    "reservoir_ready_l":47,
    "water_change":[{"cron":"30 2 * * sun","drain_s":112,"settle_s":30,"refill_s":600}]}"#;

fn warsaw(epoch: u64) -> Options {
    Options {
        epoch,
        zone: Zone::Warsaw,
        ..Options::COARSE.manual()
    }
}

fn local_hm(epoch: u64) -> (u8, u8, u8, u8) {
    let at = civil(Zone::Warsaw.local(epoch));
    (at.day, at.hour, at.minute, at.weekday)
}

#[derive(Default)]
struct Firings {
    top_offs: Vec<OperationRecord>,
    water_changes: Vec<OperationRecord>,
    skipped: Vec<SkippedWindow>,
}

async fn start(epoch: u64, settings: &str) -> Sim {
    let sim = Sim::start_with(Layout::REAL.with_reservoir(RESERVOIR_L), warsaw(epoch)).await;
    let patch: PumpSettingsPatch = serde_json::from_str(settings).expect("settings json");
    sim.service
        .apply_settings(patch)
        .expect("settings accepted");
    sim
}

async fn run_until_epoch(sim: &Sim, until: u64, evaporate_at: &[u64]) -> Firings {
    let mut firings = Firings::default();
    let mut last = sim.service.status();

    while sim.clock.true_epoch() < until {
        if evaporate_at.contains(&sim.clock.true_epoch()) {
            sim.water().evaporate(EVAPORATED_L);
        }

        sim.step().await;
        let status = sim.service.status();

        if status.last_top_off != last.last_top_off {
            firings.top_offs.extend(status.last_top_off);
        }
        if status.last_water_change != last.last_water_change {
            firings.water_changes.extend(status.last_water_change);
        }
        if status.last_skipped != last.last_skipped {
            firings.skipped.extend(status.last_skipped);
        }
        last = status;
    }

    assert_eq!(last.operation.state, OperationState::Idle);
    let water = sim.water();
    assert_eq!(water.spilled_l, 0.0);
    assert_eq!(water.return_dry_s, 0.0);
    assert!(water.balance_error_l().abs() < 1e-6);
    firings
}

#[test]
fn warsaw_offsets_follow_the_eu_rules() {
    assert_eq!(local_hm(utc(2026, 1, 15, 12, 0)), (15, 13, 0, 4));
    assert_eq!(local_hm(utc(2026, 7, 15, 12, 0)), (15, 14, 0, 3));

    assert_eq!(local_hm(utc(2026, 3, 29, 0, 59)), (29, 1, 59, 0));
    assert_eq!(local_hm(utc(2026, 3, 29, 1, 0)), (29, 3, 0, 0));
    assert_eq!(local_hm(utc(2026, 3, 22, 1, 0)), (22, 2, 0, 0));

    assert_eq!(local_hm(utc(2026, 10, 25, 0, 30)), (25, 2, 30, 0));
    assert_eq!(local_hm(utc(2026, 10, 25, 0, 59)), (25, 2, 59, 0));
    assert_eq!(local_hm(utc(2026, 10, 25, 1, 0)), (25, 2, 0, 0));
    assert_eq!(local_hm(utc(2026, 10, 25, 1, 30)), (25, 2, 30, 0));
    assert_eq!(local_hm(utc(2026, 11, 1, 1, 30)), (1, 2, 30, 0));
}

#[tokio::test(start_paused = true)]
async fn dst_fall_back_fires_a_0230_top_off_once() {
    LocalSet::new()
        .run_until(async {
            let first = utc(2026, 10, 25, 0, 30);
            let repeated = utc(2026, 10, 25, 1, 30);
            let sim = start(utc(2026, 10, 24, 22, 0), TOP_OFF_AT_0230).await;

            let firings = run_until_epoch(
                &sim,
                utc(2026, 10, 25, 3, 0),
                &[utc(2026, 10, 25, 0, 0), utc(2026, 10, 25, 1, 0)],
            )
            .await;

            let started: Vec<_> = firings
                .top_offs
                .iter()
                .map(|record| record.started_at)
                .collect();
            assert_eq!(started, [Some(first)]);
            assert_eq!(firings.top_offs[0].result, OperationResult::SumpFull);
            assert_eq!(local_hm(first), local_hm(repeated));
            assert!(firings.skipped.is_empty());

            let water = sim.water();
            assert_eq!(water.count(Event::Pump(PumpId::Makeup, true)), 1);
            assert!((water.added_l - EVAPORATED_L).abs() < 0.5);
            assert!((water.deficit_l() - EVAPORATED_L).abs() < 0.5);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn dst_fall_back_fires_each_minute_of_a_two_minute_window_once() {
    LocalSet::new()
        .run_until(async {
            let matches = [
                utc(2026, 10, 25, 0, 0),
                utc(2026, 10, 25, 0, 30),
                utc(2026, 10, 25, 1, 0),
                utc(2026, 10, 25, 1, 30),
            ];
            let sim = start(utc(2026, 10, 24, 22, 0), TOP_OFF_AT_0200_AND_0230).await;
            let evaporations: Vec<u64> = matches.iter().map(|at| at - 600).collect();

            let firings = run_until_epoch(&sim, utc(2026, 10, 25, 3, 0), &evaporations).await;

            let started: Vec<_> = firings
                .top_offs
                .iter()
                .map(|record| record.started_at)
                .collect();
            assert_eq!(started, [Some(matches[0]), Some(matches[1])]);
            assert_eq!(local_hm(matches[0]), local_hm(matches[2]));
            assert_eq!(local_hm(matches[1]), local_hm(matches[3]));
            assert!(firings.skipped.is_empty());

            let water = sim.water();
            println!("two-minute window over the fall-back: {}", water.describe());
            assert_eq!(water.count(Event::Pump(PumpId::Makeup, true)), 2);
            assert!((water.added_l - 2.0 * EVAPORATED_L).abs() < 0.5);
            assert!((water.deficit_l() - 2.0 * EVAPORATED_L).abs() < 0.5);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_every_minute_window_skipped_in_summer_time_stays_skipped_in_winter_time() {
    LocalSet::new()
        .run_until(async {
            let first = utc(2026, 10, 25, 0, 0);
            let repeat = first + 3600;
            let settings = r#"{"top_off":[{"cron":"* 2 * * *","duration_s":5}]}"#;
            let sim = start(first - 60, settings).await;
            let firings = run_until_epoch(&sim, repeat, &[]).await;
            assert_eq!(firings.skipped.len(), 60);
            let firings = run_until_epoch(&sim, repeat + 3600, &[repeat]).await;
            assert!(firings.top_offs.is_empty());
            assert!(firings.skipped.is_empty());
            assert_eq!(sim.water().count(Event::Pump(PumpId::Makeup, true)), 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn dst_spring_forward_never_fires_a_0230_top_off() {
    LocalSet::new()
        .run_until(async {
            let sim = start(utc(2026, 3, 28, 23, 0), TOP_OFF_AT_0230).await;

            let firings = run_until_epoch(
                &sim,
                utc(2026, 3, 30, 1, 0),
                &[utc(2026, 3, 28, 23, 0), utc(2026, 3, 29, 12, 0)],
            )
            .await;

            let started: Vec<_> = firings
                .top_offs
                .iter()
                .map(|record| record.started_at)
                .collect();
            assert_eq!(started, [Some(utc(2026, 3, 30, 0, 30))]);
            assert_eq!(local_hm(utc(2026, 3, 30, 0, 30)), (30, 2, 30, 1));
            assert!(firings.skipped.is_empty());
            assert_eq!(sim.water().count(Event::Pump(PumpId::Makeup, true)), 1);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn dst_fall_back_runs_a_0230_water_change_once_without_a_skip() {
    LocalSet::new()
        .run_until(async {
            let sim = start(utc(2026, 10, 24, 22, 0), WATER_CHANGE_AT_0230).await;
            let patch: PumpSettingsPatch =
                serde_json::from_str(r#"{"min_water_change_gap_h":0}"#).expect("settings json");
            sim.service
                .apply_settings(patch)
                .expect("settings accepted");

            let firings = run_until_epoch(&sim, utc(2026, 10, 25, 3, 0), &[]).await;

            assert_eq!(firings.water_changes.len(), 1);
            let record = firings.water_changes[0];
            assert_eq!(record.started_at, Some(utc(2026, 10, 25, 0, 30)));
            assert_eq!(record.result, OperationResult::SumpFull);
            assert_eq!(record.drain_ran_s, 112);
            assert!(firings.skipped.is_empty());
            assert_eq!(sim.water().count(Event::Pump(PumpId::Drain, true)), 1);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn dst_spring_forward_loses_a_0230_water_change_without_a_skip_record() {
    LocalSet::new()
        .run_until(async {
            let sim = start(utc(2026, 3, 28, 23, 0), WATER_CHANGE_AT_0230).await;

            let firings = run_until_epoch(&sim, utc(2026, 3, 29, 4, 0), &[]).await;

            assert!(firings.water_changes.is_empty());
            assert!(firings.skipped.is_empty());
            assert_eq!(sim.water().count(Event::Pump(PumpId::Drain, true)), 0);
        })
        .await;
}
