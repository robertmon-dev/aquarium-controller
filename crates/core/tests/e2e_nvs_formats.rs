#![recursion_limit = "256"]

mod common;
mod sim;

use std::sync::Arc;
use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    ChannelMode, OperationKind, OperationResult, OperationState, PumpId, PumpSettings,
};
use aquarium_core::domain::water::WaterLevelRole;
use aquarium_core::ports::inbound::{PumpControl, StartError};
use aquarium_core::ports::outbound::PumpRecord;

use sim::{Aquarium, Event, Layout, Options, Sim, SimStore, BASE_EPOCH};

const RESERVOIR_L: f64 = 150.0;
const LONG: Duration = Duration::from_secs(1800);

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

async fn boot_with(records: &[(PumpRecord, &str)]) -> Sim {
    let store = Arc::new(SimStore::default());
    for (record, json) in records {
        store.seed(*record, json.as_bytes());
    }

    let sim = Sim::start_seeded(
        Aquarium::new(Layout::REAL.with_reservoir(RESERVOIR_L)),
        Options::DEFAULT,
        store,
    )
    .await;
    sim.run_for(secs(3)).await;
    sim
}

fn assert_circulating(sim: &Sim, label: &str) {
    let water = sim.water();
    assert!(
        water.pump_on(PumpId::Return),
        "{label}: {}",
        water.describe()
    );
    assert!(!water.pump_on(PumpId::Drain), "{label}");
    assert!(!water.pump_on(PumpId::Makeup), "{label}");
    assert_eq!(water.spilled_l, 0.0, "{label}");
}

#[tokio::test(start_paused = true)]
async fn settings_from_before_the_refill_safeguards_load_with_defaults() {
    LocalSet::new()
        .run_until(async {
            let sim = boot_with(&[(
                PumpRecord::Settings,
                r#"{"top_off":[{"cron":"0 8 * * *","duration_s":60}],
                    "water_change":[{"cron":"0 10 * * sun","drain_s":100,"refill_s":300}],
                    "flow_lph":{"makeup":600},"resume_window_s":600}"#,
            )])
            .await;

            let settings = sim.service.settings();
            assert_eq!(settings.top_off.len(), 1);
            assert_eq!(settings.water_change[0].settle_s, 30);
            assert_eq!(settings.flow_lph.drain, None);
            assert_eq!(
                settings.refill_stop_sensor,
                Some(WaterLevelRole::SumpReturnLevel)
            );
            assert_eq!(settings.min_water_change_gap_h, 12);
            assert_eq!(settings.reservoir_ready_l, None);
            assert_circulating(&sim, "old settings");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn unreadable_or_invalid_settings_fall_back_to_the_defaults() {
    LocalSet::new()
        .run_until(async {
            for (label, json) in [
                (
                    "unknown stop sensor",
                    r#"{"top_off":[{"cron":"0 8 * * *","duration_s":60}],"water_change":[],
                        "flow_lph":{},"resume_window_s":600,"refill_stop_sensor":"sump_overflow"}"#,
                ),
                ("truncated", r#"{"top_off":[{"cron":"0 8 * * *","dur"#),
                ("garbage", "\u{0}\u{1}not json"),
                ("wrong shape", r#"[1,2,3]"#),
                (
                    "out of range",
                    r#"{"top_off":[],"water_change":[],"flow_lph":{},"resume_window_s":9999}"#,
                ),
            ] {
                let sim = boot_with(&[(PumpRecord::Settings, json)]).await;
                assert_eq!(sim.service.settings(), PumpSettings::DEFAULT, "{label}");
                assert_circulating(&sim, label);
            }
        })
        .await;
}

const OLD_CHECKPOINT: &str = r#"{"operation":{"plan":{"kind":"water_change","drain_s":60,
    "settle_s":30,"refill_s":600},"phase":"draining","ran_ms":[20000,0,0],
    "started_at":1795341400,"resumed":false},"checkpoint_at":1795341470}"#;

#[tokio::test(start_paused = true)]
async fn a_checkpoint_without_refill_wall_time_still_resumes() {
    LocalSet::new()
        .run_until(async {
            const { assert!(BASE_EPOCH - 1_795_341_470 < 600) };
            let sim = boot_with(&[(PumpRecord::Checkpoint, OLD_CHECKPOINT)]).await;
            assert_eq!(
                sim.service.status().operation.state,
                OperationState::Draining
            );

            sim.run_until(LONG, "the resumed water change to finish", |status, _| {
                status.operation.state == OperationState::Idle && status.last_water_change.is_some()
            })
            .await;
            let record = sim.service.status().last_water_change.expect("a record");
            assert!(record.resumed);
            assert_eq!(record.drain_ran_s, 60);
            assert_eq!(record.result, OperationResult::SumpFull);
            assert!((sim.water().drained_l - 40.0 * 1500.0 / 3600.0).abs() < 0.6);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unreadable_checkpoint_boots_idle() {
    LocalSet::new()
        .run_until(async {
            for (label, json) in [
                (
                    "unknown phase",
                    r#"{"operation":{"plan":{"kind":"water_change","drain_s":60,"settle_s":30,
                        "refill_s":600},"phase":"flooding","ran_ms":[0,0,0],"started_at":null,
                        "resumed":false},"checkpoint_at":null}"#,
                ),
                (
                    "unknown kind",
                    r#"{"operation":{"plan":{"kind":"partial_change","drain_s":60,"settle_s":30,
                        "refill_s":600},"phase":"draining","ran_ms":[0,0,0],"started_at":null,
                        "resumed":false},"checkpoint_at":null}"#,
                ),
                ("missing ran_ms", r#"{"operation":{"plan":{"kind":"top_off","drain_s":0,"settle_s":0,"refill_s":60},"phase":"refilling","started_at":null,"resumed":false},"checkpoint_at":null}"#),
                ("truncated", &OLD_CHECKPOINT[..80]),
                ("tombstone", "null"),
                ("garbage", "\u{ff}"),
            ] {
                let sim = boot_with(&[(PumpRecord::Checkpoint, json)]).await;
                assert_eq!(
                    sim.service.status().operation.state,
                    OperationState::Idle,
                    "{label}"
                );
                sim.run_for(secs(30)).await;
                assert_circulating(&sim, label);
                assert_eq!(sim.water().count(Event::Pump(PumpId::Drain, true)), 0);
            }
        })
        .await;
}

const OLD_HISTORY: &str = r#"{"top_off":null,"water_change":{"kind":"water_change",
    "started_at":1795334400,"drain_ran_s":112,"settle_ran_s":30,"refill_ran_s":290,
    "result":"sump_full","resumed":false}}"#;

#[tokio::test(start_paused = true)]
async fn history_without_the_drain_mark_still_enforces_the_gap() {
    LocalSet::new()
        .run_until(async {
            let sim = boot_with(&[(PumpRecord::LastOperations, OLD_HISTORY)]).await;
            let status = sim.service.status();
            assert_eq!(
                status
                    .last_water_change
                    .map(|record| (record.result, record.drain_ran_s)),
                Some((OperationResult::SumpFull, 112))
            );
            let (code, response) = sim
                .send(
                    "POST",
                    "/api/water-change",
                    r#"{"drain_s":60,"refill_s":600}"#,
                )
                .await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("recent"), "got: {response}");
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unsynced_cancel_does_not_erase_the_gap_of_an_old_history() {
    LocalSet::new()
        .run_until(async {
            let mut sim = boot_with(&[(PumpRecord::LastOperations, OLD_HISTORY)]).await;
            sim.clock.lose_sync();
            sim.service
                .start_water_change(aquarium_core::domain::pump::WaterChangePlan {
                    drain_s: 60,
                    settle_s: 30,
                    refill_s: 600,
                })
                .expect("the gap needs a synced clock");
            sim.service
                .cancel(OperationKind::WaterChange)
                .expect("cancelled");

            sim.restart(secs(30), true).await;
            sim.run_for(secs(3)).await;
            let (code, response) = sim
                .send(
                    "POST",
                    "/api/water-change",
                    r#"{"drain_s":60,"refill_s":600}"#,
                )
                .await;
            assert_eq!(code, 423, "got: {response}");
            assert!(response.contains("recent"), "got: {response}");
            assert_eq!(sim.water().count(Event::Pump(PumpId::Drain, true)), 0);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn an_unknown_result_drops_only_its_entry_and_keeps_the_gap() {
    LocalSet::new()
        .run_until(async {
            for (label, json, kept) in [
                (
                    "unknown top-off result",
                    r#"{"top_off":{"kind":"top_off","started_at":1795334000,"drain_ran_s":0,
                        "settle_ran_s":0,"refill_ran_s":60,"result":"exploded","resumed":false},
                        "water_change":{"kind":"water_change","started_at":1795334400,
                        "drain_ran_s":112,"settle_ran_s":30,"refill_ran_s":290,
                        "result":"sump_full","resumed":false}}"#,
                    true,
                ),
                (
                    "unknown water-change result",
                    r#"{"top_off":null,"water_change":{"kind":"water_change",
                        "started_at":1795334400,"drain_ran_s":112,"settle_ran_s":30,
                        "refill_ran_s":290,"result":"exploded","resumed":false},
                        "drained_at":1795334400}"#,
                    false,
                ),
            ] {
                let sim = boot_with(&[(PumpRecord::LastOperations, json)]).await;
                let status = sim.service.status();
                assert_eq!(status.last_top_off, None, "{label}");
                assert_eq!(status.last_water_change.is_some(), kept, "{label}");
                let (code, response) = sim
                    .send(
                        "POST",
                        "/api/water-change",
                        r#"{"drain_s":60,"refill_s":600}"#,
                    )
                    .await;
                assert_eq!(code, 423, "{label}: {response}");
                assert!(response.contains("recent"), "{label}: {response}");
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn unreadable_history_loses_the_gap_and_the_last_results() {
    LocalSet::new()
        .run_until(async {
            for (label, json) in [
                ("truncated", &OLD_HISTORY[..60]),
                ("garbage", "~"),
                ("wrong shape", "[1,2]"),
            ] {
                let sim = boot_with(&[(PumpRecord::LastOperations, json)]).await;
                let status = sim.service.status();
                assert_eq!(status.last_water_change, None, "{label}");
                assert_eq!(status.last_top_off, None, "{label}");
                let (code, response) = sim
                    .send(
                        "POST",
                        "/api/water-change",
                        r#"{"drain_s":60,"refill_s":600}"#,
                    )
                    .await;
                assert_eq!(code, 202, "{label}: {response}");
                sim.service
                    .cancel(OperationKind::WaterChange)
                    .expect("cancelled");
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn override_records_of_every_shape_boot_without_a_panic() {
    LocalSet::new()
        .run_until(async {
            for (label, json, uv_off) in [
                ("indefinite uv", r#"[null,{"expires_at":null}]"#, true),
                ("uv without the field", r#"[null,{}]"#, false),
                (
                    "malformed uv expiry",
                    r#"[null,{"expires_at":"later"}]"#,
                    false,
                ),
                (
                    "malformed return slot",
                    r#"[{"expires_at":-1},{"expires_at":null}]"#,
                    true,
                ),
                ("expired", r#"[{"expires_at":1},{"expires_at":1}]"#, false),
                ("one slot", r#"[null]"#, false),
                ("three slots", r#"[null,null,null]"#, false),
                (
                    "three slots with uv",
                    r#"[null,{"expires_at":null},null]"#,
                    true,
                ),
                (
                    "unknown field",
                    r#"[null,{"expires_at":null,"reason":"x"}]"#,
                    true,
                ),
                ("truncated", r#"[null,{"expires_"#, false),
                ("garbage", "\u{0}", false),
            ] {
                let sim = boot_with(&[(PumpRecord::Overrides, json)]).await;
                let uv = sim.service.status().channels.uv;
                assert_eq!(uv.mode == ChannelMode::ManualOff, uv_off, "{label}");
                assert_eq!(sim.water().pump_on(PumpId::Uv), !uv_off, "{label}");
                assert!(sim.water().pump_on(PumpId::Return), "{label}");
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn a_garbage_runtime_record_restarts_the_counters() {
    LocalSet::new()
        .run_until(async {
            let sim = boot_with(&[
                (PumpRecord::Runtime, r#"{"runtime_s":[1,2,3]}"#),
                (PumpRecord::Settings, "{"),
                (PumpRecord::Checkpoint, "{"),
                (PumpRecord::LastOperations, "{"),
                (PumpRecord::Overrides, "{"),
            ])
            .await;
            let status = sim.service.status();
            assert!(status.channels.return_pump.runtime_s <= 3);
            assert_eq!(status.operation.state, OperationState::Idle);
            assert_circulating(&sim, "all garbage");
            assert_eq!(
                sim.service.start_top_off(60),
                Err(StartError::Blocked(
                    aquarium_core::domain::pump::Blocker::SumpFull
                ))
            );
        })
        .await;
}
