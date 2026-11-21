#![recursion_limit = "256"]

mod common;
mod sim;

use std::time::Duration;

use tokio::task::LocalSet;

use aquarium_core::domain::pump::{
    Blocker, ChannelMode, ChannelStatus, OperationKind, OperationRecord, OperationResult,
    OperationState, PumpId, PumpSettings, PumpsStatus, WaterChangePlan,
};
use aquarium_core::domain::water::WaterLevelRole;
use aquarium_core::ports::inbound::PumpControl;

use sim::{Event, Layout, Sim};

const RESERVOIR_L: f64 = 150.0;
const TOP_OFF_DEFICIT_L: f64 = 4.0;
const LITRES_TOL: f64 = 0.6;
const LEVEL_TOL_CM: f64 = 0.3;
const BALANCE_TOL: f64 = 1e-6;
const FLOW_TOL_L: f32 = 0.01;

const WATER_CHANGE: &str = "/api/water-change";
const WATER_CHANGE_RESUME: &str = "/api/water-change/resume";
const TOP_OFF: &str = "/api/pumps/top-off";
const TOP_OFF_RESUME: &str = "/api/pumps/top-off/resume";
const SETTINGS: &str = "/api/pumps/settings";
const RETURN_OVERRIDE: &str = "/api/pumps/return/override";
const UV_OVERRIDE: &str = "/api/pumps/uv/override";

const PLAN: &str = r#"{"drain_s":60,"settle_s":60,"refill_s":600}"#;
const TOP_OFF_PLAN: &str = r#"{"duration_s":300}"#;

const LONG: Duration = Duration::from_secs(1800);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Draining,
    Settling,
    Refilling,
    TopOff,
}

impl Phase {
    const ALL: [Self; 4] = [
        Self::Draining,
        Self::Settling,
        Self::Refilling,
        Self::TopOff,
    ];

    const fn kind(self) -> OperationKind {
        match self {
            Self::TopOff => OperationKind::TopOff,
            _ => OperationKind::WaterChange,
        }
    }

    const fn path(self) -> &'static str {
        match self {
            Self::TopOff => TOP_OFF,
            _ => WATER_CHANGE,
        }
    }

    const fn state(self) -> OperationState {
        match self {
            Self::Draining => OperationState::Draining,
            Self::Settling => OperationState::Settling,
            Self::Refilling | Self::TopOff => OperationState::Refilling,
        }
    }
}

fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}

fn record(status: &PumpsStatus, kind: OperationKind) -> Option<OperationRecord> {
    match kind {
        OperationKind::TopOff => status.last_top_off,
        OperationKind::WaterChange => status.last_water_change,
    }
}

fn sorted(mut codes: Vec<u16>) -> Vec<u16> {
    codes.sort_unstable();
    codes
}

async fn accepted(sim: &Sim, method: &str, path: &str, json: &str, expected: u16) -> String {
    let (code, response) = sim.send(method, path, json).await;
    assert_eq!(code, expected, "{method} {path}: {response}");
    response
}

async fn start_sim() -> Sim {
    let sim = Sim::start(Layout::REAL.with_reservoir(RESERVOIR_L)).await;
    accepted(
        &sim,
        "PATCH",
        SETTINGS,
        r#"{"flow_lph":{"makeup":600,"drain":1500},"refill_stop_sensor":"sump_return_level"}"#,
        200,
    )
    .await;
    sim
}

async fn start_in(phase: Phase) -> Sim {
    let sim = start_sim().await;

    if phase == Phase::TopOff {
        sim.water().evaporate(TOP_OFF_DEFICIT_L);
        sim.run_for(secs(1)).await;
        accepted(&sim, "POST", TOP_OFF, TOP_OFF_PLAN, 202).await;
    } else {
        accepted(&sim, "POST", WATER_CHANGE, PLAN, 202).await;
    }

    sim.run_until(LONG, "the phase", |status, _| {
        status.operation.state == phase.state()
    })
    .await;

    match phase {
        Phase::Draining | Phase::Settling => sim.run_for(secs(30)).await,
        Phase::Refilling => {
            sim.run_until(LONG, "makeup to run mid-refill", |status, _| {
                status.operation.refill_ran_s >= 20
            })
            .await;
        }
        Phase::TopOff => {
            sim.run_until(LONG, "makeup to run mid-top-off", |status, _| {
                status.operation.refill_ran_s >= 5
            })
            .await;
        }
    }

    let status = sim.service.status();
    assert_eq!(status.operation.state, phase.state(), "{phase:?}");
    assert_eq!(status.operation.kind, Some(phase.kind()));
    sim
}

async fn finish(sim: &Sim, kind: OperationKind) -> OperationRecord {
    let before = record(&sim.service.status(), kind);
    sim.run_until(LONG, "the operation to finish", |status, _| {
        status.operation.state == OperationState::Idle && record(status, kind) != before
    })
    .await;
    record(&sim.service.status(), kind).expect("a finished operation")
}

fn assert_safe(sim: &Sim, phase: Phase) {
    let water = sim.water();
    assert_eq!(water.spilled_l, 0.0, "{phase:?}: {}", water.describe());
    assert_eq!(water.tank_spilled_l, 0.0, "{phase:?}");
    assert_eq!(water.return_dry_s, 0.0, "{phase:?}");
    assert_eq!(water.makeup_dry_s, 0.0, "{phase:?}");
    assert!(
        water.balance_error_l().abs() < BALANCE_TOL,
        "{phase:?}: balance off by {}",
        water.balance_error_l()
    );
}

#[tokio::test(start_paused = true)]
async fn api_cancel_mid_phase_ends_the_operation_for_good() {
    LocalSet::new()
        .run_until(async {
            for phase in Phase::ALL {
                let sim = start_in(phase).await;
                let drained_before = sim.water().drained_l;

                accepted(&sim, "DELETE", phase.path(), "", 204).await;
                let cancelled_at = sim.water().t_s;
                let status = sim.service.status();
                assert_eq!(status.operation.state, OperationState::Idle, "{phase:?}");
                let last = record(&status, phase.kind()).expect("a record");
                assert_eq!(last.result, OperationResult::Aborted, "{phase:?}");
                assert!(!last.resumed);

                accepted(&sim, "DELETE", phase.path(), "", 204).await;
                accepted(&sim, "DELETE", TOP_OFF, "", 204).await;
                accepted(&sim, "DELETE", WATER_CHANGE, "", 204).await;
                assert_eq!(record(&sim.service.status(), phase.kind()), Some(last));

                sim.run_for(secs(120)).await;
                let status = sim.status().await;
                assert_eq!(status.operation.state, OperationState::Idle);
                assert!(!status.channels.drain.on && !status.channels.makeup.on);
                {
                    let water = sim.water();
                    assert_eq!(
                        water.count_after(Event::Pump(PumpId::Drain, true), cancelled_at),
                        0
                    );
                    assert_eq!(
                        water.count_after(Event::Pump(PumpId::Makeup, true), cancelled_at),
                        0
                    );
                    assert!(water.drained_l - drained_before < 0.5);
                    assert!(water.tank_l <= water.layout.tank_weir_l);
                }
                assert_safe(&sim, phase);

                match phase {
                    Phase::TopOff => {
                        accepted(&sim, "POST", TOP_OFF, r#"{"duration_s":60}"#, 202).await;
                    }
                    _ => {
                        let response = accepted(&sim, "POST", WATER_CHANGE, PLAN, 423).await;
                        assert!(response.contains("recent"), "got: {response}");
                    }
                }
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn api_overrides_mid_phase_pause_only_the_refill() {
    LocalSet::new()
        .run_until(async {
            for phase in Phase::ALL {
                let sim = start_in(phase).await;

                let response =
                    accepted(&sim, "PUT", RETURN_OVERRIDE, r#"{"duration_s":600}"#, 200).await;
                let (_, payload) = response.split_once("\r\n\r\n").expect("body");
                let channel: ChannelStatus = serde_json::from_str(payload.trim()).expect("json");
                assert_eq!(channel.mode, ChannelMode::ManualOff);
                accepted(&sim, "PUT", UV_OVERRIDE, "{}", 200).await;

                sim.run_for(secs(2)).await;
                let before = sim.service.status();
                let added_before = sim.water().added_l;
                sim.run_for(secs(20)).await;
                let during = sim.service.status();

                assert_eq!(during.operation.state, phase.state(), "{phase:?}");
                assert!(!during.channels.return_pump.on && !during.channels.uv.on);
                assert_eq!(during.channels.uv.blocked_by, Some(Blocker::ReturnOff));
                assert_eq!(during.channels.uv.mode, ChannelMode::ManualOff);
                match phase {
                    Phase::Draining => {
                        assert!(during.operation.drain_ran_s >= before.operation.drain_ran_s + 19);
                        assert!(during.channels.drain.on);
                    }
                    Phase::Settling => {
                        assert!(
                            during.operation.settle_ran_s >= before.operation.settle_ran_s + 19
                        );
                    }
                    Phase::Refilling | Phase::TopOff => {
                        assert_eq!(during.operation.refill_ran_s, before.operation.refill_ran_s);
                        assert_eq!(during.channels.makeup.blocked_by, Some(Blocker::ReturnOff));
                        assert_eq!(sim.water().added_l, added_before);
                    }
                }

                accepted(&sim, "DELETE", RETURN_OVERRIDE, "", 204).await;
                accepted(&sim, "DELETE", UV_OVERRIDE, "", 204).await;

                let last = finish(&sim, phase.kind()).await;
                assert_eq!(last.result, OperationResult::SumpFull, "{phase:?}");
                sim.run_for(secs(60)).await;
                let status = sim.service.status();
                assert_eq!(status.channels.return_pump.mode, ChannelMode::Auto);
                assert_eq!(status.channels.uv.mode, ChannelMode::Auto);
                {
                    let water = sim.water();
                    assert!(water.pump_on(PumpId::Return) && water.pump_on(PumpId::Uv));
                    assert!(water.deficit_l().abs() < LITRES_TOL, "{}", water.describe());
                }
                assert_safe(&sim, phase);
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn api_settings_patch_mid_phase_moves_the_stop_sensor_and_the_litres() {
    LocalSet::new()
        .run_until(async {
            for phase in Phase::ALL {
                let sim = start_in(phase).await;

                accepted(
                    &sim,
                    "PATCH",
                    SETTINGS,
                    r#"{"refill_stop_sensor":"sump_mechanical","flow_lph":{"makeup":600,"drain":1200}}"#,
                    200,
                )
                .await;
                let status = sim.status().await;
                let expected = status.operation.drain_ran_s as f32 * 1200.0 / 3600.0;
                assert!(
                    (status.operation.drained_l.expect("drain flow") - expected).abs() < FLOW_TOL_L,
                    "{phase:?}: {:?}",
                    status.operation
                );

                accepted(
                    &sim,
                    "PATCH",
                    SETTINGS,
                    r#"{"refill_stop_sensor":"main_tank"}"#,
                    400,
                )
                .await;
                accepted(
                    &sim,
                    "PATCH",
                    SETTINGS,
                    r#"{"top_off":[{"cron":"61 * * * *","duration_s":60}]}"#,
                    400,
                )
                .await;
                let settings: PumpSettings = sim.fetch(SETTINGS).await;
                assert_eq!(
                    settings.refill_stop_sensor,
                    Some(WaterLevelRole::SumpMechanical)
                );
                assert_eq!(settings.flow_lph.drain, Some(1200));
                assert!(settings.top_off.is_empty());

                let last = finish(&sim, phase.kind()).await;
                assert_eq!(last.result, OperationResult::SumpFull, "{phase:?}");
                {
                    let water = sim.water();
                    let layout = water.layout;
                    assert!(
                        (water.return_level_cm() - layout.sump_mechanical.height_cm).abs()
                            < LEVEL_TOL_CM,
                        "{phase:?}: {}",
                        water.describe()
                    );
                    assert!(
                        (-water.deficit_l()
                            - layout.headroom_to_level_l(layout.sump_mechanical.height_cm))
                        .abs()
                            < LITRES_TOL,
                        "{phase:?}: overshoot {:.2} L",
                        -water.deficit_l()
                    );
                }
                assert_safe(&sim, phase);

                accepted(
                    &sim,
                    "PATCH",
                    SETTINGS,
                    r#"{"refill_stop_sensor":"sump_return_level"}"#,
                    200,
                )
                .await;
                sim.run_for(secs(1)).await;
                let response = accepted(&sim, "POST", TOP_OFF, r#"{"duration_s":60}"#, 423).await;
                assert!(response.contains("full"), "got: {response}");
            }
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn api_conflicting_starts_from_two_clients_admit_one_operation() {
    LocalSet::new()
        .run_until(async {
            let sim = start_sim().await;
            let (a, b) = tokio::join!(
                sim.send("POST", WATER_CHANGE, PLAN),
                sim.send("POST", WATER_CHANGE, PLAN)
            );
            assert_eq!(sorted(vec![a.0, b.0]), [202, 409], "{a:?} / {b:?}");
            sim.run_for(secs(2)).await;
            assert_eq!(
                sim.service.status().operation.state,
                OperationState::Draining
            );
            assert_eq!(sim.water().count(Event::Pump(PumpId::Drain, true)), 1);

            for phase in [Phase::Draining, Phase::Settling, Phase::Refilling] {
                sim.run_until(LONG, "the phase", |status, _| {
                    status.operation.state == phase.state()
                })
                .await;
                sim.run_for(secs(5)).await;

                let (a, b) = tokio::join!(
                    sim.send("POST", TOP_OFF, TOP_OFF_PLAN),
                    sim.send("POST", WATER_CHANGE, PLAN)
                );
                assert_eq!((a.0, b.0), (409, 409), "{phase:?}: {a:?} / {b:?}");
                let (a, b) = tokio::join!(
                    sim.send("POST", WATER_CHANGE_RESUME, ""),
                    sim.send("POST", TOP_OFF_RESUME, "")
                );
                assert_eq!((a.0, b.0), (409, 409), "{phase:?}: {a:?} / {b:?}");
                assert_eq!(sim.service.status().operation.state, phase.state());
            }

            let last = finish(&sim, OperationKind::WaterChange).await;
            assert_eq!(last.result, OperationResult::SumpFull);
            assert_safe(&sim, Phase::Refilling);

            let sim = start_sim().await;
            accepted(
                &sim,
                "PATCH",
                SETTINGS,
                r#"{"min_water_change_gap_h":0}"#,
                200,
            )
            .await;
            accepted(&sim, "POST", WATER_CHANGE, PLAN, 202).await;
            sim.run_for(secs(10)).await;
            let (cancel, restart) = tokio::join!(
                sim.send("DELETE", WATER_CHANGE, ""),
                sim.send("POST", WATER_CHANGE, PLAN)
            );
            assert_eq!(cancel.0, 204, "{cancel:?}");
            assert!(
                [202, 409].contains(&restart.0),
                "cancel vs start: {restart:?}"
            );
            sim.run_for(secs(2)).await;
            let status = sim.service.status();
            let draining = sim.water().pump_on(PumpId::Drain);
            if restart.0 == 202 {
                assert_eq!(status.operation.state, OperationState::Draining);
                assert!(draining);
            } else {
                assert_eq!(status.operation.state, OperationState::Idle);
                assert!(!draining);
            }

            let sim = start_sim().await;
            sim.water().evaporate(TOP_OFF_DEFICIT_L);
            sim.run_for(secs(1)).await;
            accepted(&sim, "POST", TOP_OFF, TOP_OFF_PLAN, 202).await;
            sim.run_for(secs(3)).await;
            let (change, top_off) = tokio::join!(
                sim.send("POST", WATER_CHANGE, PLAN),
                sim.send("POST", TOP_OFF, TOP_OFF_PLAN)
            );
            assert_eq!(
                (change.0, top_off.0),
                (202, 409),
                "{change:?} / {top_off:?}"
            );
            sim.run_for(secs(2)).await;
            let status = sim.service.status();
            assert_eq!(status.operation.kind, Some(OperationKind::WaterChange));
            assert_eq!(status.operation.state, OperationState::Draining);
            assert_eq!(
                status.last_top_off.map(|record| record.result),
                Some(OperationResult::Aborted)
            );
            {
                let water = sim.water();
                assert!(!water.pump_on(PumpId::Makeup) && water.pump_on(PumpId::Drain));
            }
            let last = finish(&sim, OperationKind::WaterChange).await;
            assert_eq!(last.result, OperationResult::SumpFull);
            assert_safe(&sim, Phase::TopOff);
        })
        .await;
}

#[tokio::test(start_paused = true)]
async fn api_water_change_cancelled_before_it_drained_does_not_start_the_gap() {
    LocalSet::new()
        .run_until(async {
            let sim = start_sim().await;
            sim.run_for(secs(5)).await;
            sim.service
                .start_water_change(WaterChangePlan {
                    drain_s: 60,
                    settle_s: 60,
                    refill_s: 600,
                })
                .expect("idle");
            sim.service.cancel(OperationKind::WaterChange);
            let cancelled = sim.service.status().last_water_change.expect("a record");
            assert_eq!(cancelled.result, OperationResult::Aborted);
            assert_eq!(cancelled.drain_ran_s, 0);

            sim.run_for(secs(5)).await;
            accepted(&sim, "POST", WATER_CHANGE, PLAN, 202).await;
            let done = finish(&sim, OperationKind::WaterChange).await;
            assert_eq!(done.result, OperationResult::SumpFull);
            assert_eq!(done.drain_ran_s, 60);

            sim.run_for(secs(5)).await;
            let response = accepted(&sim, "POST", WATER_CHANGE, PLAN, 423).await;
            assert!(response.contains("recent"), "got: {response}");

            let water = sim.water();
            assert_eq!(water.count(Event::Pump(PumpId::Drain, true)), 1);
            assert!((water.drained_l - 25.0).abs() < LITRES_TOL);
            drop(water);
            assert_safe(&sim, Phase::Draining);
        })
        .await;
}

const SCHEDULED: &str = r#"{
    "water_change":[{"cron":"* * * * *","drain_s":60,"settle_s":60,"refill_s":600}],
    "top_off":[{"cron":"* * * * *","duration_s":300}]}"#;

const RESCHEDULED: &str = r#"{
    "water_change":[{"cron":"0 0 1 1 *","drain_s":1,"settle_s":0,"refill_s":1}],
    "top_off":[{"cron":"0 0 1 1 *","duration_s":1}],
    "flow_lph":{"makeup":300,"drain":750}}"#;

async fn start_scheduled_in(phase: Phase) -> Sim {
    let sim = start_sim().await;
    let only_top_off = r#"{"water_change":[],"top_off":[{"cron":"* * * * *","duration_s":300}]}"#;

    if phase == Phase::TopOff {
        sim.water().evaporate(TOP_OFF_DEFICIT_L);
        accepted(&sim, "PATCH", SETTINGS, only_top_off, 200).await;
    } else {
        accepted(&sim, "PATCH", SETTINGS, SCHEDULED, 200).await;
    }

    sim.run_until(LONG, "the scheduled phase", |status, _| {
        status.operation.state == phase.state() && status.operation.kind == Some(phase.kind())
    })
    .await;
    sim.run_for(secs(3)).await;
    sim
}

#[tokio::test(start_paused = true)]
async fn api_settings_patch_mid_phase_keeps_the_running_plan() {
    LocalSet::new()
        .run_until(async {
            for phase in Phase::ALL {
                let sim = start_scheduled_in(phase).await;
                let before = sim.status().await.operation;

                accepted(&sim, "PATCH", SETTINGS, RESCHEDULED, 200).await;
                let after = sim.status().await.operation;
                assert_eq!(after.state, before.state, "{phase:?}");
                assert!(
                    after.phase_remaining_s.expect("running")
                        >= before.phase_remaining_s.expect("running").saturating_sub(1),
                    "{phase:?}: {before:?} -> {after:?}"
                );
                assert!(
                    (after.refilled_l.expect("makeup flow")
                        - after.refill_ran_s as f32 * 300.0 / 3600.0)
                        .abs()
                        < FLOW_TOL_L,
                    "{phase:?}: {after:?}"
                );

                let last = finish(&sim, phase.kind()).await;
                assert_eq!(
                    last.result,
                    OperationResult::SumpFull,
                    "{phase:?}: {last:?}"
                );
                match phase {
                    Phase::TopOff => assert!(last.refill_ran_s > 1, "{phase:?}: {last:?}"),
                    _ => {
                        assert_eq!(last.drain_ran_s, 60, "{phase:?}");
                        assert_eq!(last.settle_ran_s, 60, "{phase:?}");
                        assert!(last.refill_ran_s > 1, "{phase:?}: {last:?}");
                    }
                }
                assert_safe(&sim, phase);

                let settings: PumpSettings = sim.fetch(SETTINGS).await;
                assert_eq!(settings.water_change.first().map(|w| w.drain_s), Some(1));
                assert_eq!(settings.flow_lph.makeup, Some(300));
            }
        })
        .await;
}
